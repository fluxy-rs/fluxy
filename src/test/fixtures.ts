import type {
  AppSnapshot,
  ChatConversationView,
  ChatMessageView,
  ClipboardShareView,
  EngineStatusView,
  NetworkStatusView,
  PeerSummary,
  RegisteredFileView,
  SettingsSnapshot,
  TransferSummary,
} from "../bindings";
import type { PairingChallenge } from "../state/engine";

const NOW = 1_792_650_000_000;

export function makeStatus(overrides: Partial<EngineStatusView> = {}): EngineStatusView {
  return {
    version: "0.2.0",
    protocol_version: 1,
    device_id: "device-local",
    device_name: "Local Device",
    data_dir: "C:\\Users\\test\\AppData\\Roaming\\fluxy",
    listen_port: 53000,
    trusted_device_count: 1,
    dev_loopback: false,
    platform: "windows",
    identity_fingerprint: "00".repeat(32),
    ...overrides,
  };
}

export function makeSettings(overrides: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
  return {
    device_name: "Local Device",
    listen_port: 53000,
    discovery_port: 42420,
    clipboard_max_bytes: 1_048_576,
    auto_clipboard: false,
    max_active_transfers: 2,
    notifications: true,
    auto_accept_transfers: false,
    autostart: false,
    autostart_location: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\\Fluxy",
    receive_dir: "C:\\Users\\test\\AppData\\Roaming\\fluxy\\received",
    receive_dir_ready: true,
    dev_loopback: false,
    ...overrides,
  };
}

export function makeNetwork(overrides: Partial<NetworkStatusView> = {}): NetworkStatusView {
  return {
    state: "discovering",
    detail: "mDNS discovery is active on the selected LAN interface.",
    interface: "Ethernet",
    local_address: "192.168.1.20",
    listen_addr: "192.168.1.20:53000",
    discovery_port: 42420,
    mdns_service: "_fluxy._udp.local.",
    advertised: true,
    browsing: true,
    nearby_count: 0,
    local_addresses: ["Ethernet 192.168.1.20"],
    dev_loopback: false,
    selection_mode: "auto",
    selection_source: "default_route",
    interface_index: 7,
    interface_pinned: true,
    rebind_count: 0,
    last_rebind_ms: null,
    interface_options: [
      {
        name: "Ethernet",
        label: "Ethernet",
        kind: "ethernet",
        index: 7,
        up: true,
        virtual_adapter: false,
        default_route: true,
        addresses: ["192.168.1.20"],
        recommended: true,
      },
      {
        name: "Wi-Fi",
        label: "Wi-Fi",
        kind: "wireless",
        index: 12,
        up: true,
        virtual_adapter: false,
        default_route: false,
        addresses: ["10.0.0.5"],
        recommended: false,
      },
    ],
    fallback_active: true,
    fallback_gateway: "192.168.1.1",
    fallback_targets: ["192.168.1.255:42420", "255.255.255.255:42420"],
    fallback_probes_sent: 1,
    fallback_responses: 1,
    fallback_peers: 0,
    fallback_last_error: null,
    ...overrides,
  };
}

export function makePeer(overrides: Partial<PeerSummary> = {}): PeerSummary {
  return {
    device_id: "peer-1",
    name: "Peer One",
    platform: "linux",
    trusted: true,
    manufacturer: "LENOVO",
    model: "21LF",
    os_version: "Windows 11 Pro (26H2)",
    online: true,
    clipboard_policy: "ask",
    auto_accept: "inherit",
    paired_at_ms: NOW - 60_000,
    last_seen_ms: NOW,
    addresses: ["192.168.1.21:53001"],
    ...overrides,
  };
}

export function makeConversation(
  overrides: Partial<ChatConversationView> = {},
): ChatConversationView {
  return {
    device_id: "peer-1",
    name: "Peer One",
    platform: "linux",
    online: true,
    session_state: "connected",
    last_message: null,
    last_at_ms: 0,
    ...overrides,
  };
}

export function makeMessage(overrides: Partial<ChatMessageView> = {}): ChatMessageView {
  return {
    message_id: "message-1",
    device_id: "peer-1",
    direction: "sent",
    text: "Hello",
    state: "delivered",
    created_at_ms: NOW,
    sequence: 1,
    copy_hint: false,
    error_code: null,
    error_message: null,
    retryable: false,
    ...overrides,
  };
}

export function makeShare(overrides: Partial<ClipboardShareView> = {}): ClipboardShareView {
  return {
    offer_id: "offer-1",
    device_id: "peer-1",
    direction: "received",
    state: "offered",
    preview: "clipboard preview",
    size_bytes: 16,
    created_at_ms: Date.now(),
    expires_at_ms: Date.now() + 120_000,
    error_code: null,
    error_message: null,
    ...overrides,
  };
}

export function makeRegisteredFile(
  overrides: Partial<RegisteredFileView> = {},
): RegisteredFileView {
  return {
    file_id: "file-1",
    name: "report.bin",
    size: 5,
    ...overrides,
  };
}

export function makeTransfer(overrides: Partial<TransferSummary> = {}): TransferSummary {
  return {
    transfer_id: "transfer-1",
    device_id: "peer-1",
    device_name: "Peer One",
    direction: "send",
    state: "transferring",
    file_name: "report.bin",
    bytes_total: 2048,
    bytes_done: 1024,
    bytes_per_second: 1024 * 1024,
    total_files: 1,
    batch_id: null,
    batch_root: null,
    relative_path: null,
    error_code: null,
    error_code_number: null,
    error_message: null,
    retryable: false,
    created_at_ms: NOW,
    updated_at_ms: NOW,
    ...overrides,
  };
}

export function makeSnapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    snapshot_version: 1,
    seq: 1,
    status: makeStatus(),
    peers: [makePeer()],
    transfers: [],
    settings: makeSettings(),
    network: makeNetwork(),
    chat: {
      conversations: [makeConversation()],
      messages: [],
    },
    clipboard: { shares: [] },
    ...overrides,
  };
}

export function makePairing(overrides: Partial<PairingChallenge> = {}): PairingChallenge {
  return {
    challenge_id: "challenge-1",
    peer_device_id: "peer-1",
    peer_name: "Peer One",
    peer_platform: "linux",
    direction: "initiator",
    sas: "123456",
    expires_at_ms: Date.now() + 120_000,
    ...overrides,
  };
}
