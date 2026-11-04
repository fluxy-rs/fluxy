import { Channel, invoke } from "@tauri-apps/api/core";
import {
  commands,
  type AppEventEnvelope,
  type AppSnapshot,
  type ChatMessageView,
  type ClipboardReceivePolicy,
  type ClipboardShareView,
  type EngineStatusView,
  type IpcError,
  type NetworkStatusView,
  type PairingChallengeView,
  type RegisteredFileView,
  type RegisteredFolderView,
  type SettingsSnapshot,
  type ShellLabels,
  type NearbyPeerView,
  type SubscriptionInfo,
  type SystemInsets,
  type TransferAction,
  type TransferSummary,
} from "../bindings";

export class EngineApiError extends Error {
  readonly code: string;
  readonly codeNumber: number;
  readonly retryable: boolean;

  constructor(error: IpcError) {
    super(error.message);
    this.name = "EngineApiError";
    this.code = error.code;
    this.codeNumber = error.code_number;
    this.retryable = error.retryable;
  }
}

type CommandResult<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: IpcError };

function unwrap<T>(result: CommandResult<T>): T {
  if (result.status === "ok") {
    return result.data;
  }
  throw new EngineApiError(result.error);
}

/// The device a tapped notification belongs to, once. Null when none was
/// recorded; the caller falls back to the message list.
export async function takeOpenChatDevice(): Promise<string | null> {
  if (!isDesktopRuntime()) return null;
  const result = await invoke<{ device: string | null }>("plugin:fluxy-mobile|open_chat", {});
  return result.device;
}

export function isDesktopRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in window
  );
}

export async function getEngineStatus(): Promise<EngineStatusView> {
  return unwrap(await commands.engineStatus());
}

export async function getStateSnapshot(): Promise<AppSnapshot> {
  return unwrap(await commands.stateSnapshot());
}

export async function rotateIdentity(): Promise<EngineStatusView> {
  return unwrap(await commands.identityRotate());
}

export async function startPairing(deviceId: string): Promise<PairingChallengeView> {
  return unwrap(await commands.pairStart(deviceId));
}

export async function confirmPairing(
  challengeId: string,
  accepted: boolean,
): Promise<void> {
  unwrap(await commands.pairConfirm(challengeId, accepted));
}

export async function cancelPairing(challengeId: string): Promise<void> {
  unwrap(await commands.pairCancel(challengeId));
}

export async function removeTrust(deviceId: string): Promise<boolean> {
  return unwrap(await commands.trustRemove(deviceId));
}

export async function setShellLabels(labels: ShellLabels): Promise<void> {
  unwrap(await commands.shellSetLabels(labels));
}

export async function getDiscoveryDiagnostics(): Promise<NetworkStatusView> {
  return unwrap(await commands.discoveryDiagnostics());
}

/**
 * Choose which interface and address the listener binds. "auto" follows the
 * default route and re-selects on a network change; "manual" pins one interface
 * and address, and reports 2003 when that address disappears instead of
 * silently moving. Switching drops current sessions and pauses active
 * transfers, which resume from their checkpoints.
 */
export async function setNetworkSelection(
  mode: "auto" | "manual",
  interfaceId: string | null,
  address: string | null,
): Promise<NetworkStatusView> {
  return unwrap(await commands.setNetworkSelection(mode, interfaceId, address));
}

export async function openChat(deviceId: string): Promise<void> {
  unwrap(await commands.chatOpen(deviceId));
}

export async function sendChatMessage(
  deviceId: string,
  text: string,
): Promise<ChatMessageView> {
  return unwrap(await commands.chatSend(deviceId, text));
}

export async function retryChatMessage(
  deviceId: string,
  messageId: string,
): Promise<ChatMessageView> {
  return unwrap(await commands.chatRetry(deviceId, messageId));
}

export async function shareClipboard(deviceId: string): Promise<ClipboardShareView> {
  return unwrap(await commands.clipboardShare(deviceId));
}

export async function acceptClipboard(
  deviceId: string,
  offerId: string,
): Promise<ClipboardShareView> {
  return unwrap(await commands.clipboardAccept(deviceId, offerId));
}

export async function rejectClipboard(
  deviceId: string,
  offerId: string,
): Promise<ClipboardShareView> {
  return unwrap(await commands.clipboardReject(deviceId, offerId));
}

export async function setClipboardPolicy(
  deviceId: string,
  policy: ClipboardReceivePolicy,
): Promise<void> {
  unwrap(await commands.clipboardSetPolicy(deviceId, policy));
}

export async function pickFile(
  title: string | null,
): Promise<RegisteredFileView | null> {
  return unwrap(await commands.pickFile(title));
}

/** Pick one folder and register it for a batch send. */
export async function pickFolder(
  title: string | null,
): Promise<RegisteredFolderView | null> {
  return unwrap(await commands.pickFolder(title));
}

/** Queue one transfer per file of a registered folder. */
export async function startFolder(
  deviceId: string,
  folderId: string,
): Promise<TransferSummary[]> {
  return unwrap(await commands.startFolder(deviceId, folderId));
}

export async function pickFiles(
  title: string | null,
): Promise<RegisteredFileView[]> {
  return unwrap(await commands.pickFiles(title));
}

export async function startTransfers(
  deviceId: string,
  fileIds: string[],
): Promise<TransferSummary[]> {
  return unwrap(await commands.startTransfers(deviceId, fileIds));
}

export async function setTransferConcurrency(
  limit: number,
): Promise<SettingsSnapshot> {
  return unwrap(await commands.setTransferConcurrency(limit));
}

/** Paint the window behind the status bar to match the page background. */
export async function setChromeColor(color: string): Promise<void> {
  unwrap(await commands.setChromeColor(color));
}

/** System bar insets, so a mobile page pads itself out of the OS chrome. */
export async function systemInsets(): Promise<SystemInsets> {
  return unwrap(await commands.systemInsets());
}

/** Devices discovery has seen, paired or not. */
export async function nearbyPeers(): Promise<NearbyPeerView[]> {
  return (await commands.nearbyPeers()) ?? [];
}

/** Pair with an address and fingerprint the user typed. */
export async function pairManual(
  address: string,
  fingerprint: string,
): Promise<PairingChallengeView> {
  return unwrap(await commands.pairManual(address, fingerprint));
}

/** Open the platform picker and register the chosen file (mobile only). */
export async function pickSourceFile(): Promise<RegisteredFileView | null> {
  const result = unwrap(await commands.pickSourceFile());
  return result ?? null;
}

/**
 * Copy a received file into the platform's public downloads (mobile only).
 * Returns the URI the platform handed back.
 */
export async function exportFile(
  path: string,
  name: string,
  mime: string,
): Promise<string> {
  return invoke<string>("plugin:fluxy-mobile|fs_export", { path, name, mime });
}

/** Register a content URI that was already handed to us (mobile only). */
export async function openSourceFile(uri: string): Promise<RegisteredFileView | null> {
  // Goes through the shell, not the plugin: the plugin only opens the
  // descriptor, and a file without an id cannot be sent.
  return unwrap(await commands.openSharedSource(uri));
}

/** Recent application log lines, oldest first. */
export async function appLogTail(limit = 200): Promise<string[]> {
  return unwrap(await commands.appLogTail(limit));
}

/** Whether Android lets the app use the local network (mobile only). */
export async function localNetworkGranted(): Promise<boolean> {
  const state = await invoke<{ granted: boolean }>(
    "plugin:fluxy-mobile|local_network_granted",
  );
  return state.granted;
}

/** Files shared into the app, plus any device a deep link named (mobile only). */
export async function sharedFiles(): Promise<{
  uris: string[];
  device: string | null;
}> {
  const batch = await invoke<{
    files?: { uri: string }[];
    device?: string | null;
  }>("plugin:fluxy-mobile|shared_files");
  return {
    uris: (batch.files ?? []).map((entry) => entry.uri),
    device: batch.device ?? null,
  };
}

/** Free bytes on the volume that receives files (mobile only). */
export async function storageAvailable(): Promise<number> {
  return invoke<number>("plugin:fluxy-mobile|storage_available");
}

/** Choose where received files land. An empty value restores the default. */
export async function setReceiveDir(
  directory: string | null,
): Promise<SettingsSnapshot> {
  return unwrap(await commands.setReceiveDir(directory));
}

/** Open the folder picker; null means the dialog was dismissed. */
export async function pickReceiveDir(title: string | null): Promise<string | null> {
  return unwrap(await commands.pickReceiveDir(title));
}

export async function setDeviceName(name: string): Promise<SettingsSnapshot> {
  return unwrap(await commands.setDeviceName(name));
}

export async function setAutoAccept(enabled: boolean): Promise<SettingsSnapshot> {
  return unwrap(await commands.setAutoAccept(enabled));
}

export async function setNotifications(enabled: boolean): Promise<SettingsSnapshot> {
  return unwrap(await commands.setNotifications(enabled));
}

/**
 * Turn the automatic clipboard watcher on or off. It is off by default: when it
 * is on the backend reads the clipboard every 300 ms and offers a stable,
 * locally-authored value to online trusted devices.
 */
export async function setAutoClipboard(
  enabled: boolean,
): Promise<SettingsSnapshot> {
  return unwrap(await commands.setAutoClipboard(enabled));
}

/**
 * Register or remove the login item. The login item starts the app with
 * --hidden, so a login start lands in the tray and keeps receiving.
 */
export async function setAutostart(enabled: boolean): Promise<SettingsSnapshot> {
  return unwrap(await commands.setAutostart(enabled));
}

export async function registerFile(path: string): Promise<RegisteredFileView> {
  return unwrap(await commands.registerFile(path));
}

export async function startTransfer(
  deviceId: string,
  fileId: string,
): Promise<TransferSummary> {
  return unwrap(await commands.startTransfer(deviceId, fileId));
}

export async function respondTransfer(
  transferId: string,
  accepted: boolean,
): Promise<TransferSummary> {
  return unwrap(await commands.transferRespond(transferId, accepted));
}

export async function controlTransfer(
  transferId: string,
  action: TransferAction,
): Promise<TransferSummary> {
  return unwrap(await commands.transferControl(transferId, action));
}

export async function listTransfers(): Promise<TransferSummary[]> {
  return unwrap(await commands.transfersList());
}

export async function subscribeEvents(
  onEvent: (event: AppEventEnvelope) => void,
): Promise<SubscriptionInfo> {
  const channel = new Channel<AppEventEnvelope>();
  channel.onmessage = onEvent;
  return unwrap(await commands.eventsSubscribe(channel));
}
