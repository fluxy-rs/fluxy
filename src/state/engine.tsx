import { pendingOffers } from "../lib/offers";
import {
  createContext,
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  useContext,
  type Accessor,
  type ParentProps,
} from "solid-js";
import type {
  AppEventEnvelope,
  AppSnapshot,
  ChatConversationView,
  ChatMessageView,
  ClipboardReceivePolicy,
  ClipboardShareView,
  DeviceAutoAccept,
  EngineStatusView,
  NearbyPeerView,
  NetworkStatusView,
  RegisteredFileView,
  RegisteredFolderView,
  SettingsSnapshot,
  SystemInsets,
  TransferAction,
  TransferSummary,
} from "../bindings";
import {
  EngineApiError,
  acceptClipboard as acceptClipboardCommand,
  cancelPairing as cancelPairingCommand,
  confirmPairing as confirmPairingCommand,
  controlTransfer as controlTransferCommand,
  getStateSnapshot,
  isDesktopRuntime,
  openChat as openChatCommand,
  pickFile as pickFileCommand,
  pickFiles as pickFilesCommand,
  pickFolder as pickFolderCommand,
  startFolder as startFolderCommand,
  registerFile as registerFileCommand,
  rejectClipboard as rejectClipboardCommand,
  removeTrust as removeTrustCommand,
  openReceived as openReceivedCommand,
  respondTransfer as respondTransferCommand,
  setDeviceName as setDeviceNameCommand,
  pairManual as pairManualCommand,
  nearbyPeers as nearbyPeersCommand,
  pickSourceFile as pickSourceFileCommand,
  exportFile as exportFileCommand,
  storageAvailable as storageAvailableCommand,
  sharedFiles as sharedFilesCommand,
  openSourceFile as openSourceFileCommand,
  systemInsets as systemInsetsCommand,
  setChromeColor as setChromeColorCommand,
  setReceiveDir as setReceiveDirCommand,
  pickReceiveDir as pickReceiveDirCommand,
  setNetworkSelection as setNetworkSelectionCommand,
  setNotifications as setNotificationsCommand,
  setAutoAccept as setAutoAcceptCommand,
  setAutoClipboard as setAutoClipboardCommand,
  setAutostart as setAutostartCommand,
  setTransferConcurrency as setTransferConcurrencyCommand,
  retryChatMessage as retryChatCommand,
  rotateIdentity,
  sendChatMessage as sendChatCommand,
  setClipboardPolicy as setClipboardPolicyCommand,
  setDeviceAutoAccept as setDeviceAutoAcceptCommand,
  setShellLabels as setShellLabelsCommand,
  shareClipboard as shareClipboardCommand,
  startPairing as startPairingCommand,
  startTransfer as startTransferCommand,
  appLogTail as appLogTailCommand,
  localNetworkGranted as localNetworkGrantedCommand,
  startTransfers as startTransfersCommand,
  subscribeEvents,
} from "../api/engine";
import { useI18n } from "../i18n";

export type EngineFailure = {
  code: string;
  message: string;
  detail?: string;
  retryable?: boolean;
};

export type PairingChallenge = {
  challenge_id: string;
  peer_device_id: string;
  peer_name: string;
  peer_platform: string;
  direction: string;
  sas: string;
  expires_at_ms: number;
};

export type PairingNotice =
  | { kind: "succeeded"; device_name: string }
  | { kind: "failed"; code: string; message: string };

type PairingEventData = Extract<AppEventEnvelope["event"], { category: "pairing" }>["data"];
type ChatEventData = Extract<AppEventEnvelope["event"], { category: "chat" }>["data"];
type ClipboardEventData = Extract<AppEventEnvelope["event"], { category: "clipboard" }>["data"];
type TransferEventData = Extract<AppEventEnvelope["event"], { category: "transfer" }>["data"];
/** A navigation request raised by native chrome such as the tray. */
export type ShellRequest = Extract<AppEventEnvelope["event"], { category: "shell" }>["data"];

/** A batch of files and folders the OS dropped on the window. */
export interface TransferDrop {
  files: RegisteredFileView[];
  /** Dropped folders, already scanned; each becomes a batch. */
  folders: RegisteredFolderView[];
  rejected: string[];
  skipped: number;
}

// Exported so the Vitest mock engine can provide the same contract without
// starting Tauri. Production components consume it through useEngine().
export type EngineContextValue = {
  snapshot: Accessor<AppSnapshot | null>;
  lastEvent: Accessor<AppEventEnvelope | null>;
  eventCount: Accessor<number>;
  error: Accessor<EngineFailure | null>;
  loading: Accessor<boolean>;
  desktop: boolean;
  pairing: Accessor<PairingChallenge | null>;
  pairingNotice: Accessor<PairingNotice | null>;
  pairingBusy: Accessor<boolean>;
  pairingError: Accessor<EngineFailure | null>;
  chatConversations: Accessor<ChatConversationView[]>;
  chatMessages: Accessor<ChatMessageView[]>;
  clipboardShares: Accessor<ClipboardShareView[]>;
  transfers: Accessor<TransferSummary[]>;
  /** Set when native chrome (the tray) asks the UI to show a route. */
  shellRequest: Accessor<ShellRequest | null>;
  clearShellRequest: () => void;
  incomingOffer: Accessor<TransferSummary | null>;
  /**
   * Every offer the prompt is currently answering. A batch arrives as one
   * offer per file, and asking the receiver to confirm each separately is
   * asking the same question N times.
   */
  incomingOffers: Accessor<TransferSummary[]>;
  transferBusy: Accessor<string | null>;
  transferError: Accessor<EngineFailure | null>;
  dragActive: Accessor<boolean>;
  drop: Accessor<TransferDrop | null>;
  refresh: () => Promise<void>;
  rotate: () => Promise<EngineStatusView>;
  pairStart: (deviceId: string) => Promise<void>;
  pairConfirm: () => Promise<void>;
  pairCancel: () => Promise<void>;
  removeTrust: (deviceId: string) => Promise<void>;
  openChat: (deviceId: string) => Promise<void>;
  sendChat: (deviceId: string, text: string) => Promise<ChatMessageView>;
  retryChat: (deviceId: string, messageId: string) => Promise<ChatMessageView>;
  shareClipboard: (deviceId: string) => Promise<ClipboardShareView>;
  setClipboardPolicy: (deviceId: string, policy: ClipboardReceivePolicy) => Promise<void>;
  setDeviceAutoAccept: (deviceId: string, policy: DeviceAutoAccept) => Promise<void>;
  acceptClipboard: (deviceId: string, offerId: string) => Promise<ClipboardShareView>;
  rejectClipboard: (deviceId: string, offerId: string) => Promise<ClipboardShareView>;
  pickFile: (title: string | null) => Promise<RegisteredFileView | null>;
  registerFile: (path: string) => Promise<RegisteredFileView>;
  startTransfer: (deviceId: string, fileId: string) => Promise<TransferSummary>;
  pickFiles: (title: string | null) => Promise<RegisteredFileView[]>;
  pickFolder: (title: string | null) => Promise<RegisteredFolderView | null>;
  startFolder: (deviceId: string, folderId: string) => Promise<TransferSummary[]>;
  startTransfers: (deviceId: string, fileIds: string[]) => Promise<TransferSummary[]>;
  setTransferConcurrency: (limit: number) => Promise<SettingsSnapshot>;
  setDeviceName: (name: string) => Promise<SettingsSnapshot>;
  pairManual: (address: string, fingerprint: string) => Promise<PairingChallenge>;
  nearbyPeers: () => Promise<NearbyPeerView[]>;
  pickSourceFile: () => Promise<RegisteredFileView | null>;
  /** Copy a received file into the platform downloads (mobile only). */
  exportFile: (path: string, name: string, mime: string) => Promise<string>;
  /** Free bytes on the receiving volume (mobile only). */
  storageAvailable: () => Promise<number>;
  /** Whether Android lets the app use the local network. */
  localNetworkGranted: () => Promise<boolean>;

  /** Recent application log lines, oldest first. */
  appLogTail: (limit?: number) => Promise<string[]>;

  /** Shared URIs plus any device a deep link named (mobile only). */
  sharedFiles: () => Promise<{ uris: string[]; device: string | null }>;
  /** Register a content URI we were handed (mobile only). */
  openSourceFile: (uri: string) => Promise<RegisteredFileView | null>;
  systemInsets: () => Promise<SystemInsets>;
  setChromeColor: (color: string) => Promise<void>;
  setReceiveDir: (directory: string | null) => Promise<SettingsSnapshot>;
  pickReceiveDir: (title: string | null) => Promise<string | null>;
  setNotifications: (enabled: boolean) => Promise<SettingsSnapshot>;
  setAutoAccept: (enabled: boolean) => Promise<SettingsSnapshot>;
  setAutoClipboard: (enabled: boolean) => Promise<SettingsSnapshot>;
  setAutostart: (enabled: boolean) => Promise<SettingsSnapshot>;
  setNetworkSelection: (
    mode: "auto" | "manual",
    interfaceId: string | null,
    address: string | null,
  ) => Promise<NetworkStatusView>;
  clearDrop: () => void;
  openReceivedTransfer: (transferId: string) => Promise<void>;
  respondTransfer: (transferId: string, accepted: boolean) => Promise<TransferSummary>;
  controlTransfer: (transferId: string, action: TransferAction) => Promise<TransferSummary>;
  clearTransferError: () => void;
  clearPairingNotice: () => void;
};

export const EngineContext = createContext<EngineContextValue>();

function toFailure(cause: unknown): EngineFailure {
  if (cause instanceof EngineApiError) {
    return {
      code: cause.code,
      message: cause.message,
      retryable: cause.retryable,
    };
  }
  if (cause instanceof Error) {
    return { code: "unknown", message: cause.message };
  }
  return { code: "unknown", message: String(cause) };
}

export function EngineProvider(props: ParentProps) {
  const [snapshot, setSnapshot] = createSignal<AppSnapshot | null>(null);
  const [lastEvent, setLastEvent] = createSignal<AppEventEnvelope | null>(null);
  const [eventCount, setEventCount] = createSignal(0);
  const [error, setError] = createSignal<EngineFailure | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [pairing, setPairing] = createSignal<PairingChallenge | null>(null);
  const [pairingNotice, setPairingNotice] = createSignal<PairingNotice | null>(null);
  const [pairingBusy, setPairingBusy] = createSignal(false);
  const [pairingError, setPairingError] = createSignal<EngineFailure | null>(null);
  const [shellRequest, setShellRequest] = createSignal<ShellRequest | null>(null);
  const [transferBusy, setTransferBusy] = createSignal<string | null>(null);
  const [transferError, setTransferError] = createSignal<EngineFailure | null>(null);
  const [dragActive, setDragActive] = createSignal(false);
  const [drop, setDrop] = createSignal<TransferDrop | null>(null);
  const chatConversations = () => snapshot()?.chat.conversations ?? [];
  const chatMessages = () => snapshot()?.chat.messages ?? [];
  const clipboardShares = () => snapshot()?.clipboard.shares ?? [];
  const transfers = () => snapshot()?.transfers ?? [];
  const incomingOffers = () => pendingOffers(transfers());
  const incomingOffer = () => incomingOffers()[0] ?? null;
  const chatBuffers = new Map<string, Partial<ChatMessageView>>();
  let pairingTimer: number | undefined;

  let expectedSeq = 0;
  let currentSnapshotVersion = 0;
  let refreshing: Promise<void> | null = null;
  let refreshPending = false;
  let disposed = false;

  const desktop = isDesktopRuntime();
  const { t } = useI18n();

  createEffect(() => {
    if (!desktop) return;
    void setShellLabelsCommand({
      open: t("tray.open"),
      quit: t("tray.quit"),
      autoClipboard: t("tray.autoClipboard"),
      autostart: t("tray.autostart"),
      help: t("tray.help"),
      chatTitle: t("notification.chatTitle"),
      chatBody: t("notification.chatBody"),
      clipboardTitle: t("notification.clipboardTitle"),
      clipboardBody: t("notification.clipboardBody"),
      clipboardAppliedTitle: t("notification.clipboardAppliedTitle"),
      clipboardAppliedBody: t("notification.clipboardAppliedBody"),
      transferTitle: t("notification.transferTitle"),
      transferBody: t("notification.transferBody"),
      transferFailedTitle: t("notification.transferFailedTitle"),
      transferFailedBody: t("notification.transferFailedBody"),
    }).catch(() => {
      // Native labels are non-critical; the app remains usable without them.
    });
  });

  async function refreshInternal(initial: boolean): Promise<void> {
    if (!desktop) {
      setError({
        code: "desktop_runtime",
        message: "The local engine is only available in the desktop shell.",
        detail: "Run bun run tauri:dev to start the Tauri shell.",
      });
      setLoading(false);
      return;
    }

    if (refreshing) {
      // Coalesce concurrent callers, but do not let an older in-flight
      // snapshot satisfy a newer event. Run once more after it settles.
      refreshPending = true;
      return refreshing;
    }

    if (initial) {
      setLoading(true);
    }

    const task = (async () => {
      try {
        const next = mergeChatBuffers(await getStateSnapshot());
        if (disposed) {
          return;
        }
        if (next.snapshot_version < currentSnapshotVersion) {
          // A response captured before the latest chat event must not roll
          // delivered/failed messages back to sending.
          return;
        }
        setSnapshot(next);
        pruneChatBuffers(next);
        expectedSeq = Math.max(expectedSeq, next.seq);
        currentSnapshotVersion = Math.max(currentSnapshotVersion, next.snapshot_version);
        setError(null);
      } catch (cause) {
        if (!disposed) {
          setError(toFailure(cause));
        }
      } finally {
        if (!disposed) {
          setLoading(false);
        }
      }
    })();
    refreshing = task;

    try {
      await task;
    } finally {
      refreshing = null;
    }

    if (refreshPending && !disposed) {
      refreshPending = false;
      void refreshInternal(false);
    }
  }

  function clearPairingTimer() {
    if (pairingTimer !== undefined) {
      window.clearTimeout(pairingTimer);
      pairingTimer = undefined;
    }
  }

  function applyPairingEvent(data: PairingEventData) {
    switch (data.type) {
      case "sas_ready": {
        clearPairingTimer();
        setPairing({
          challenge_id: data.challenge_id,
          peer_device_id: data.peer_device_id,
          peer_name: data.peer_name,
          peer_platform: data.peer_platform,
          direction: data.direction,
          sas: data.sas,
          expires_at_ms: data.expires_at_ms,
        });
        setPairingBusy(false);
        setPairingError(null);
        const delay = Math.max(0, data.expires_at_ms - Date.now()) + 100;
        pairingTimer = window.setTimeout(() => {
          const current = pairing();
          if (current?.challenge_id === data.challenge_id) {
            void cancelPairingCommand(data.challenge_id).catch(() => undefined);
            setPairing(null);
            setPairingNotice({ kind: "failed", code: "pairing_expired", message: "" });
          }
        }, delay);
        break;
      }
      case "succeeded": {
        clearPairingTimer();
        setPairing(null);
        setPairingBusy(false);
        setPairingNotice({ kind: "succeeded", device_name: data.device_name });
        void refreshInternal(false);
        break;
      }
      case "failed": {
        clearPairingTimer();
        setPairing(null);
        setPairingBusy(false);
        setPairingNotice({ kind: "failed", code: data.code, message: data.message });
        void refreshInternal(false);
        break;
      }
      case "cancelled": {
        clearPairingTimer();
        setPairing(null);
        setPairingBusy(false);
        break;
      }
    }
  }

  function mergeChatBuffers(next: AppSnapshot): AppSnapshot {
    if (chatBuffers.size === 0) return next;
    const messages = next.chat.messages.map((message) => {
      const patch = chatBuffers.get(message.message_id);
      if (!patch) return message;
      return { ...message, ...patch };
    });
    return { ...next, chat: { ...next.chat, messages } };
  }

  function pruneChatBuffers(next: AppSnapshot) {
    for (const [messageId, patch] of chatBuffers) {
      const message = next.chat.messages.find((item) => item.message_id === messageId);
      if (message && message.state === patch.state) {
        chatBuffers.delete(messageId);
      }
    }
  }

  function updateConversation(deviceId: string, patch: Partial<ChatConversationView>) {
    setSnapshot((previous) => {
      if (!previous) return previous;
      const conversations = previous.chat.conversations.map((item) =>
        item.device_id === deviceId ? { ...item, ...patch } : item,
      );
      return {
        ...previous,
        chat: { ...previous.chat, conversations },
      };
    });
  }

  function upsertMessage(initial: ChatMessageView) {
    const message = { ...initial, ...chatBuffers.get(initial.message_id) };
    setSnapshot((previous) => {
      if (!previous) return previous;
      chatBuffers.delete(message.message_id);
      const messages = [...previous.chat.messages];
      const index = messages.findIndex((item) => item.message_id === message.message_id);
      if (index >= 0) {
        const existing = messages[index];
        const merged = { ...existing, ...message };
        // A command response can race the delivery event. Never roll a
        // message that already reached the peer back to "sending".
        if (
          message.state === "sending" &&
          (existing.state === "delivered" || existing.state === "received")
        ) {
          merged.state = existing.state;
          merged.error_code = existing.error_code;
          merged.error_message = existing.error_message;
          merged.retryable = existing.retryable;
        }
        messages[index] = merged;
      } else {
        messages.push(message);
      }
      const conversations = previous.chat.conversations.map((item) =>
        item.device_id === message.device_id
          ? {
              ...item,
              last_message: message.text.slice(0, 96),
              last_at_ms: Math.max(item.last_at_ms, message.created_at_ms),
              online: true,
              session_state: message.direction === "received" ? "connected" : item.session_state,
            }
          : item,
      );
      return { ...previous, chat: { ...previous.chat, messages, conversations } };
    });
  }

  function patchMessage(messageId: string, patch: Partial<ChatMessageView>) {
    let found = false;
    setSnapshot((previous) => {
      if (!previous) return previous;
      const messages = previous.chat.messages.map((item) => {
        if (item.message_id !== messageId) return item;
        found = true;
        return { ...item, ...patch };
      });
      return found ? { ...previous, chat: { ...previous.chat, messages } } : previous;
    });
    if (!found) {
      chatBuffers.set(messageId, { ...chatBuffers.get(messageId), ...patch });
    }
  }

  function applyChatEvent(data: ChatEventData) {
    switch (data.type) {
      case "connecting":
        updateConversation(data.device_id, { session_state: "connecting" });
        break;
      case "connected":
        updateConversation(data.device_id, {
          session_state: "connected",
          online: true,
        });
        break;
      case "disconnected":
        updateConversation(data.device_id, { session_state: "idle" });
        break;
      case "received":
        upsertMessage({
          message_id: data.message_id,
          device_id: data.device_id,
          direction: "received",
          text: data.text,
          state: "received",
          created_at_ms: data.received_at_ms,
          sequence: 0,
          copy_hint: data.copy_hint,
          error_code: null,
          error_message: null,
          retryable: false,
        });
        break;
      case "delivered":
        patchMessage(data.message_id, { state: "delivered" });
        break;
      case "failed":
        patchMessage(data.message_id, {
          state: "failed",
          error_code: data.code,
          error_message: data.message,
          retryable: data.retryable,
        });
        break;
    }
  }

  function upsertClipboardShare(initial: ClipboardShareView) {
    setSnapshot((previous) => {
      if (!previous) return previous;
      const shares = [...previous.clipboard.shares];
      const index = shares.findIndex((item) => item.offer_id === initial.offer_id);
      if (index >= 0) {
        const existing = shares[index];
        const terminal = (state: string) =>
          state === "applied" || state === "failed" || state === "rejected" || state === "expired";
        const stale =
          (terminal(existing.state) && !terminal(initial.state)) ||
          (initial.state === "offered" && existing.state !== "offered") ||
          (initial.state === "accepted" &&
            (existing.state === "sending" || terminal(existing.state)));
        // Command responses can race the session event that already moved
        // this share forward. Never roll a newer state backwards.
        shares[index] = stale
          ? { ...existing, preview: initial.preview, size_bytes: initial.size_bytes }
          : { ...existing, ...initial };
      } else {
        shares.push(initial);
      }
      return { ...previous, clipboard: { ...previous.clipboard, shares } };
    });
  }

  function patchClipboardShare(offerId: string, patch: Partial<ClipboardShareView>): boolean {
    let found = false;
    setSnapshot((previous) => {
      if (!previous) return previous;
      const shares = previous.clipboard.shares.map((item) => {
        if (item.offer_id !== offerId) return item;
        found = true;
        return { ...item, ...patch };
      });
      return found ? { ...previous, clipboard: { ...previous.clipboard, shares } } : previous;
    });
    return found;
  }

  function applyClipboardEvent(data: ClipboardEventData) {
    switch (data.type) {
      case "offered":
        upsertClipboardShare({
          offer_id: data.offer_id,
          device_id: data.device_id,
          direction: "sent",
          state: "offered",
          preview: data.preview,
          size_bytes: data.size_bytes,
          created_at_ms: Date.now(),
          expires_at_ms: data.expires_at_ms,
          error_code: null,
          error_message: null,
        });
        break;
      case "offer_received":
        upsertClipboardShare({
          offer_id: data.offer_id,
          device_id: data.device_id,
          direction: "received",
          state: "offered",
          preview: data.text_preview,
          size_bytes: data.size_bytes,
          created_at_ms: Date.now(),
          expires_at_ms: data.expires_at_ms,
          error_code: null,
          error_message: null,
        });
        break;
      case "accepted": {
        const current = snapshot()?.clipboard.shares.find(
          (item) => item.offer_id === data.offer_id,
        );
        patchClipboardShare(data.offer_id, {
          state: current?.direction === "sent" ? "sending" : "accepted",
        });
        break;
      }
      case "applied":
        if (!patchClipboardShare(data.offer_id, { state: "applied" })) {
          void refreshInternal(false);
        }
        break;
      case "apply_failed":
        if (
          !patchClipboardShare(data.offer_id, {
            state: "failed",
            error_code: data.code,
            error_message: data.message,
          })
        ) {
          void refreshInternal(false);
        }
        break;
      case "rejected":
        patchClipboardShare(data.offer_id, { state: "rejected" });
        break;
    }
  }

  function upsertTransfer(next: TransferSummary) {
    setSnapshot((previous) => {
      if (!previous) return previous;
      const transfers = [...previous.transfers];
      const index = transfers.findIndex((item) => item.transfer_id === next.transfer_id);
      if (index >= 0) {
        transfers[index] = { ...transfers[index], ...next };
      } else {
        transfers.unshift(next);
      }
      return { ...previous, transfers };
    });
  }

  function patchTransfer(transferId: string, patch: Partial<TransferSummary>) {
    setSnapshot((previous) => {
      if (!previous) return previous;
      let found = false;
      const transfers = previous.transfers.map((item) => {
        if (item.transfer_id !== transferId) return item;
        found = true;
        return { ...item, ...patch };
      });
      return found ? { ...previous, transfers } : previous;
    });
  }

  function applyTransferEvent(data: TransferEventData) {
    switch (data.type) {
      case "offered":
        upsertTransfer({
          transfer_id: data.transfer_id,
          device_id: data.device_id,
          device_name: data.device_name,
          direction: "receive",
          state: "offered",
          file_name: data.file_name,
          bytes_total: data.file_size,
          bytes_done: 0,
          bytes_per_second: 0,
          total_files: data.total_files,
          batch_id: null,
          batch_root: null,
          relative_path: null,
          error_code: null,
          error_code_number: null,
          error_message: null,
          retryable: false,
          created_at_ms: Date.now(),
          updated_at_ms: Date.now(),
        });
        break;
      case "queued":
        upsertTransfer({
          transfer_id: data.transfer_id,
          device_id: data.device_id,
          device_name: data.device_name,
          direction: "send",
          state: "queued",
          file_name: data.file_name,
          bytes_total: data.total_bytes,
          bytes_done: 0,
          bytes_per_second: 0,
          total_files: data.total_files,
          batch_id: null,
          batch_root: null,
          relative_path: null,
          error_code: null,
          error_code_number: null,
          error_message: null,
          retryable: false,
          created_at_ms: Date.now(),
          updated_at_ms: Date.now(),
        });
        break;
      case "drag_hover":
        setDragActive(data.active);
        break;
      case "files_dropped":
        setDrop({
          files: data.files,
          folders: data.folders,
          rejected: data.rejected,
          skipped: data.skipped,
        });
        break;
      case "prepared":
        upsertTransfer({
          transfer_id: data.transfer_id,
          device_id: data.device_id,
          device_name: data.device_name,
          direction: "send",
          state: "preparing",
          file_name: data.file_name,
          bytes_total: data.total_bytes,
          bytes_done: 0,
          bytes_per_second: 0,
          total_files: data.total_files,
          batch_id: null,
          batch_root: null,
          relative_path: null,
          error_code: null,
          error_code_number: null,
          error_message: null,
          retryable: false,
          created_at_ms: Date.now(),
          updated_at_ms: Date.now(),
        });
        break;
      case "progress":
        patchTransfer(data.transfer_id, {
          state: "transferring",
          bytes_done: data.bytes_done,
          bytes_total: data.bytes_total,
          bytes_per_second: data.bytes_per_second,
          updated_at_ms: Date.now(),
        });
        break;
      case "paused":
        patchTransfer(data.transfer_id, {
          state: "paused",
          bytes_per_second: 0,
          updated_at_ms: Date.now(),
        });
        break;
      case "resumed":
        patchTransfer(data.transfer_id, {
          state: "transferring",
          updated_at_ms: Date.now(),
        });
        break;
      case "completed": {
        const current = transfers().find((item) => item.transfer_id === data.transfer_id);
        patchTransfer(data.transfer_id, {
          state: "completed",
          bytes_done: current?.bytes_total ?? current?.bytes_done ?? 0,
          bytes_per_second: 0,
          updated_at_ms: Date.now(),
        });
        break;
      }
      case "cancelled":
        patchTransfer(data.transfer_id, {
          state: "cancelled",
          bytes_per_second: 0,
          updated_at_ms: Date.now(),
        });
        break;
      case "failed":
        patchTransfer(data.transfer_id, {
          state: "failed",
          bytes_per_second: 0,
          error_code: data.code,
          error_message: data.message,
          retryable: data.retryable,
          updated_at_ms: Date.now(),
        });
        break;
      case "file_access_required":
        // File access recovery arrives with the file policy milestone (M2-10).
        break;
    }
  }

  function handleEvent(envelope: AppEventEnvelope) {
    if (disposed || envelope.seq <= expectedSeq) {
      return;
    }

    setLastEvent(envelope);
    setEventCount((count) => count + 1);

    if (envelope.event.category === "shell") {
      // Native chrome cannot navigate the WebView, so it asks the UI to.
      setShellRequest(envelope.event.data);
      expectedSeq = Math.max(expectedSeq, envelope.seq);
      currentSnapshotVersion = Math.max(currentSnapshotVersion, envelope.snapshot_version);
      return;
    }

    if (envelope.event.category === "clipboard") {
      applyClipboardEvent(envelope.event.data);
      if (envelope.seq > expectedSeq + 1) {
        void refreshInternal(false);
      }
      expectedSeq = Math.max(expectedSeq, envelope.seq);
      currentSnapshotVersion = Math.max(currentSnapshotVersion, envelope.snapshot_version);
      return;
    }

    if (envelope.event.category === "chat") {
      applyChatEvent(envelope.event.data);
      // Chat events carry the complete delta for the transcript. Applying
      // them directly avoids a snapshot round trip for every message and,
      // importantly, avoids an older in-flight snapshot overwriting a newer
      // delivered/failed state. A real gap still reconciles non-chat state.
      if (envelope.seq > expectedSeq + 1) {
        void refreshInternal(false);
      }
      expectedSeq = Math.max(expectedSeq, envelope.seq);
      currentSnapshotVersion = Math.max(currentSnapshotVersion, envelope.snapshot_version);
      return;
    }

    if (envelope.event.category === "transfer") {
      applyTransferEvent(envelope.event.data);
      // A transfer event carries the complete delta for one record. Applying
      // it directly avoids a snapshot round trip per throttled progress tick.
      if (envelope.seq > expectedSeq + 1) {
        void refreshInternal(false);
      }
      expectedSeq = Math.max(expectedSeq, envelope.seq);
      currentSnapshotVersion = Math.max(currentSnapshotVersion, envelope.snapshot_version);
      return;
    }

    if (envelope.event.category === "pairing") {
      applyPairingEvent(envelope.event.data);
    } else if (
      envelope.event.category === "peer" ||
      envelope.event.category === "trust" ||
      (envelope.event.category === "engine" && envelope.event.data.type === "network_changed")
    ) {
      void refreshInternal(false);
    }

    if (
      currentSnapshotVersion === 0 ||
      envelope.snapshot_version !== currentSnapshotVersion ||
      envelope.seq !== expectedSeq + 1
    ) {
      // A gap or version change means incremental state is no longer trusted.
      // Rebuild from state_snapshot before accepting more events.
      void refreshInternal(false);
      return;
    }

    expectedSeq = envelope.seq;
  }

  async function subscribe() {
    try {
      const info = await subscribeEvents(handleEvent);
      if (disposed) {
        return;
      }
      if (info.seq > expectedSeq || info.snapshot_version !== currentSnapshotVersion) {
        await refreshInternal(false);
      }
    } catch (cause) {
      if (!disposed) {
        setError(toFailure(cause));
      }
    }
  }

  async function rotate(): Promise<EngineStatusView> {
    const status = await rotateIdentity();
    await refreshInternal(false);
    return status;
  }

  async function pairStart(deviceId: string): Promise<void> {
    setPairingError(null);
    setPairingNotice(null);
    setPairingBusy(true);
    try {
      await startPairingCommand(deviceId);
    } catch (cause) {
      setPairingBusy(false);
      setPairingError(toFailure(cause));
    }
  }

  async function pairConfirm(): Promise<void> {
    const current = pairing();
    if (!current) return;
    setPairingBusy(true);
    setPairingError(null);
    try {
      await confirmPairingCommand(current.challenge_id, true);
    } catch (cause) {
      setPairingBusy(false);
      setPairingError(toFailure(cause));
    }
  }

  async function pairCancel(): Promise<void> {
    const current = pairing();
    clearPairingTimer();
    setPairing(null);
    setPairingBusy(false);
    setPairingError(null);
    if (!current) return;
    try {
      await cancelPairingCommand(current.challenge_id);
    } catch (cause) {
      setPairingError(toFailure(cause));
    }
  }

  async function removeTrust(deviceId: string): Promise<void> {
    await removeTrustCommand(deviceId);
    await refreshInternal(false);
  }

  async function openChat(deviceId: string): Promise<void> {
    await openChatCommand(deviceId);
  }

  async function sendChat(deviceId: string, text: string): Promise<ChatMessageView> {
    const message = await sendChatCommand(deviceId, text);
    upsertMessage(message);
    return message;
  }

  async function retryChat(deviceId: string, messageId: string): Promise<ChatMessageView> {
    const message = await retryChatCommand(deviceId, messageId);
    upsertMessage(message);
    return message;
  }

  async function shareClipboard(deviceId: string): Promise<ClipboardShareView> {
    const share = await shareClipboardCommand(deviceId);
    upsertClipboardShare(share);
    return share;
  }

  async function setClipboardPolicy(
    deviceId: string,
    policy: ClipboardReceivePolicy,
  ): Promise<void> {
    await setClipboardPolicyCommand(deviceId, policy);
    await refreshInternal(false);
  }

  async function setDeviceAutoAccept(deviceId: string, policy: DeviceAutoAccept): Promise<void> {
    await setDeviceAutoAcceptCommand(deviceId, policy);
    await refreshInternal(false);
  }

  async function acceptClipboard(deviceId: string, offerId: string): Promise<ClipboardShareView> {
    const share = await acceptClipboardCommand(deviceId, offerId);
    upsertClipboardShare(share);
    return share;
  }

  async function rejectClipboard(deviceId: string, offerId: string): Promise<ClipboardShareView> {
    const share = await rejectClipboardCommand(deviceId, offerId);
    upsertClipboardShare(share);
    return share;
  }

  async function pickFile(title: string | null): Promise<RegisteredFileView | null> {
    return pickFileCommand(title);
  }

  async function registerFile(path: string): Promise<RegisteredFileView> {
    return registerFileCommand(path);
  }

  async function startTransfer(deviceId: string, fileId: string): Promise<TransferSummary> {
    setTransferError(null);
    try {
      const summary = await startTransferCommand(deviceId, fileId);
      upsertTransfer(summary);
      return summary;
    } catch (cause) {
      setTransferError(toFailure(cause));
      throw cause;
    }
  }

  async function pickFiles(title: string | null): Promise<RegisteredFileView[]> {
    return pickFilesCommand(title);
  }

  async function pickFolder(title: string | null): Promise<RegisteredFolderView | null> {
    return pickFolderCommand(title);
  }

  async function startFolder(deviceId: string, folderId: string): Promise<TransferSummary[]> {
    setTransferError(null);
    try {
      const summaries = await startFolderCommand(deviceId, folderId);
      for (const summary of summaries) {
        upsertTransfer(summary);
      }
      return summaries;
    } catch (cause) {
      setTransferError(toFailure(cause));
      throw cause;
    }
  }

  async function startTransfers(deviceId: string, fileIds: string[]): Promise<TransferSummary[]> {
    setTransferError(null);
    try {
      const summaries = await startTransfersCommand(deviceId, fileIds);
      for (const summary of summaries) {
        upsertTransfer(summary);
      }
      return summaries;
    } catch (cause) {
      setTransferError(toFailure(cause));
      throw cause;
    }
  }

  async function setTransferConcurrency(limit: number): Promise<SettingsSnapshot> {
    const settings = await setTransferConcurrencyCommand(limit);
    setSnapshot((previous) => (previous ? { ...previous, settings } : previous));
    return settings;
  }

  async function setDeviceName(name: string): Promise<SettingsSnapshot> {
    const settings = await setDeviceNameCommand(name);
    setSnapshot((previous) => (previous ? { ...previous, settings } : previous));
    return settings;
  }

  async function nearbyPeers(): Promise<NearbyPeerView[]> {
    return nearbyPeersCommand();
  }

  async function pairManual(address: string, fingerprint: string): Promise<PairingChallenge> {
    const view = await pairManualCommand(address, fingerprint);
    // The typed view carries the challenge; the SAS arrives with the event.
    return {
      challenge_id: view.challenge_id,
      peer_device_id: view.peer_device_id,
      peer_name: view.peer_name,
      peer_platform: "",
      direction: view.direction,
      sas: "",
      expires_at_ms: 0,
    };
  }

  async function setChromeColor(color: string): Promise<void> {
    return setChromeColorCommand(color);
  }

  async function systemInsets(): Promise<SystemInsets> {
    return systemInsetsCommand();
  }

  async function pickSourceFile(): Promise<RegisteredFileView | null> {
    return pickSourceFileCommand();
  }

  async function exportFile(path: string, name: string, mime: string): Promise<string> {
    return exportFileCommand(path, name, mime);
  }

  async function storageAvailable(): Promise<number> {
    return storageAvailableCommand();
  }

  async function localNetworkGranted(): Promise<boolean> {
    return localNetworkGrantedCommand();
  }

  async function appLogTail(limit?: number): Promise<string[]> {
    return appLogTailCommand(limit);
  }

  async function sharedFiles(): Promise<{ uris: string[]; device: string | null }> {
    return sharedFilesCommand();
  }

  async function openSourceFile(uri: string): Promise<RegisteredFileView | null> {
    return openSourceFileCommand(uri);
  }

  async function setReceiveDir(directory: string | null): Promise<SettingsSnapshot> {
    const settings = await setReceiveDirCommand(directory);
    setSnapshot((previous) => (previous ? { ...previous, settings } : previous));
    return settings;
  }

  async function pickReceiveDir(title: string | null): Promise<string | null> {
    return pickReceiveDirCommand(title);
  }

  async function setAutoAccept(enabled: boolean): Promise<SettingsSnapshot> {
    const settings = await setAutoAcceptCommand(enabled);
    setSnapshot((previous) => (previous ? { ...previous, settings } : previous));
    return settings;
  }

  async function setNotifications(enabled: boolean): Promise<SettingsSnapshot> {
    const settings = await setNotificationsCommand(enabled);
    setSnapshot((previous) => (previous ? { ...previous, settings } : previous));
    return settings;
  }

  async function setAutoClipboard(enabled: boolean): Promise<SettingsSnapshot> {
    const settings = await setAutoClipboardCommand(enabled);
    setSnapshot((previous) => (previous ? { ...previous, settings } : previous));
    return settings;
  }

  async function setAutostart(enabled: boolean): Promise<SettingsSnapshot> {
    const settings = await setAutostartCommand(enabled);
    setSnapshot((previous) => (previous ? { ...previous, settings } : previous));
    return settings;
  }

  async function setNetworkSelection(
    mode: "auto" | "manual",
    interfaceId: string | null,
    address: string | null,
  ): Promise<NetworkStatusView> {
    const view = await setNetworkSelectionCommand(mode, interfaceId, address);
    // Show the new selection immediately; the rebind that follows publishes its
    // own state through the snapshot.
    setSnapshot((previous) => (previous ? { ...previous, network: view } : previous));
    await refreshInternal(false);
    return view;
  }

  function clearDrop() {
    setDrop(null);
  }

  /// Hand a completed receive to the OS so its default application opens it.
  async function openReceivedTransfer(transferId: string): Promise<void> {
    await openReceivedCommand(transferId);
  }

  async function respondTransfer(transferId: string, accepted: boolean): Promise<TransferSummary> {
    setTransferBusy(transferId);
    try {
      const summary = await respondTransferCommand(transferId, accepted);
      upsertTransfer(summary);
      return summary;
    } finally {
      setTransferBusy(null);
    }
  }

  async function controlTransfer(
    transferId: string,
    action: TransferAction,
  ): Promise<TransferSummary> {
    setTransferBusy(transferId);
    try {
      const summary = await controlTransferCommand(transferId, action);
      upsertTransfer(summary);
      return summary;
    } finally {
      setTransferBusy(null);
    }
  }

  function clearTransferError() {
    setTransferError(null);
  }

  function clearPairingNotice() {
    setPairingNotice(null);
  }

  onMount(() => {
    void (async () => {
      await refreshInternal(true);
      if (disposed || !desktop) {
        return;
      }
      if (snapshot()) {
        await subscribe();
      }
    })();
  });

  onCleanup(() => {
    disposed = true;
    clearPairingTimer();
  });

  return (
    <EngineContext.Provider
      value={{
        snapshot,
        lastEvent,
        eventCount,
        error,
        loading,
        desktop,
        pairing,
        pairingNotice,
        pairingBusy,
        pairingError,
        chatConversations,
        chatMessages,
        clipboardShares,
        transfers,
        shellRequest,
        clearShellRequest: () => setShellRequest(null),
        incomingOffer,
        incomingOffers,
        transferBusy,
        transferError,
        dragActive,
        drop,
        refresh: () => refreshInternal(false),
        rotate,
        pairStart,
        pairConfirm,
        pairCancel,
        removeTrust,
        openChat,
        sendChat,
        retryChat,
        shareClipboard,
        setClipboardPolicy,
        setDeviceAutoAccept,
        acceptClipboard,
        rejectClipboard,
        pickFile,
        registerFile,
        startTransfer,
        pickFiles,
        pickFolder,
        startFolder,
        appLogTail,
        localNetworkGranted,
        startTransfers,
        setTransferConcurrency,
        setDeviceName,
        pairManual,
        nearbyPeers,
        pickSourceFile,
        exportFile,
        storageAvailable,
        sharedFiles,
        openSourceFile,
        systemInsets,
        setChromeColor,
        setReceiveDir,
        pickReceiveDir,
        setNotifications,
        setAutoAccept,
        setAutoClipboard,
        setAutostart,
        setNetworkSelection,
        clearDrop,
        openReceivedTransfer,
        respondTransfer,
        controlTransfer,
        clearTransferError,
        clearPairingNotice,
      }}
    >
      {props.children}
    </EngineContext.Provider>
  );
}

export function useEngine(): EngineContextValue {
  const value = useContext(EngineContext);
  if (!value) {
    throw new Error("useEngine must be used inside EngineProvider");
  }
  return value;
}
