import { type ParentProps } from "solid-js";
import { vi } from "vitest";
import type {
  AppSnapshot,
  ChatMessageView,
  ClipboardShareView,
  EngineStatusView,
  NetworkStatusView,
  RegisteredFileView,
  SettingsSnapshot,
  TransferSummary,
} from "../bindings";
import { EngineContext, type EngineContextValue, type PairingChallenge } from "../state/engine";
import {
  makeMessage,
  makeNetwork,
  makeRegisteredFile,
  makeSettings,
  makeShare,
  makeStatus,
  makeTransfer,
} from "./fixtures";

export type MockEngineOptions = Partial<EngineContextValue>;

/**
 * Build a typed stand-in for the engine context. Tests only override the
 * accessors and actions they exercise; every other action is a spy that
 * resolves with a harmless fixture.
 */
export function createMockEngine(options: MockEngineOptions = {}): EngineContextValue {
  const snapshot = options.snapshot ?? (() => null as AppSnapshot | null);
  const base: EngineContextValue = {
    snapshot,
    lastEvent: () => null,
    eventCount: () => 0,
    error: () => null,
    loading: () => false,
    desktop: true,
    pairing: () => null,
    pairingNotice: () => null,
    pairingBusy: () => false,
    pairingError: () => null,
    chatConversations: () => snapshot()?.chat.conversations ?? [],
    chatMessages: () => snapshot()?.chat.messages ?? [],
    clipboardShares: () => snapshot()?.clipboard.shares ?? [],
    transfers: () => snapshot()?.transfers ?? [],
    shellRequest: () => null,
    clearShellRequest: vi.fn(),
    incomingOffer: () => null,
    openReceivedTransfer: vi.fn(),
    setDeviceAutoAccept: vi.fn(),
    transferBusy: () => null,
    transferError: () => null,
    dragActive: () => false,
    drop: () => null,
    refresh: vi.fn(async () => {}),
    rotate: vi.fn(async (): Promise<EngineStatusView> => makeStatus()),
    pairStart: vi.fn(async () => {}),
    pairConfirm: vi.fn(async () => {}),
    pairCancel: vi.fn(async () => {}),
    removeTrust: vi.fn(async () => {}),
    openChat: vi.fn(async () => {}),
    sendChat: vi.fn(async (): Promise<ChatMessageView> => makeMessage()),
    retryChat: vi.fn(async (): Promise<ChatMessageView> => makeMessage()),
    shareClipboard: vi.fn(async (): Promise<ClipboardShareView> => makeShare()),
    setClipboardPolicy: vi.fn(async () => {}),
    acceptClipboard: vi.fn(async (): Promise<ClipboardShareView> => makeShare()),
    rejectClipboard: vi.fn(async (): Promise<ClipboardShareView> => makeShare()),
    pickFile: vi.fn(async (): Promise<RegisteredFileView | null> => makeRegisteredFile()),
    registerFile: vi.fn(async (): Promise<RegisteredFileView> => makeRegisteredFile()),
    startTransfer: vi.fn(async (): Promise<TransferSummary> => makeTransfer()),
    pickFiles: vi.fn(async (): Promise<RegisteredFileView[]> => [makeRegisteredFile()]),
    pickFolder: vi.fn(async () => null),
    startFolder: vi.fn(async (): Promise<TransferSummary[]> => [makeTransfer()]),
    startTransfers: vi.fn(async (): Promise<TransferSummary[]> => [makeTransfer()]),
    setTransferConcurrency: vi.fn(async (): Promise<SettingsSnapshot> => makeSettings()),
    setDeviceName: vi.fn(async (): Promise<SettingsSnapshot> => makeSettings()),
    setAutoAccept: vi.fn(async (enabled: boolean) =>
      makeSettings({ auto_accept_transfers: enabled }),
    ),
    setNotifications: vi.fn(async (): Promise<SettingsSnapshot> => makeSettings()),
    nearbyPeers: vi.fn(async () => []),
    pairManual: vi.fn(
      async (address: string): Promise<PairingChallenge> => ({
        challenge_id: "challenge-1",
        peer_device_id: address,
        peer_name: address,
        peer_platform: "android",
        direction: "initiator",
        sas: "123456",
        expires_at_ms: Date.now() + 120_000,
      }),
    ),
    pickSourceFile: vi.fn(async () => null),
    exportFile: vi.fn(async () => "content://mock/export"),
    storageAvailable: vi.fn(async () => 1024 * 1024 * 1024),
    appLogTail: vi.fn(async () => []),
    localNetworkGranted: vi.fn(async () => true),
    sharedFiles: vi.fn(async () => ({ uris: [], device: null })),
    openSourceFile: vi.fn(async () => null),
    systemInsets: vi.fn(async () => ({ top: 0, bottom: 0 })),
    setChromeColor: vi.fn(async () => undefined),
    setReceiveDir: vi.fn(async (): Promise<SettingsSnapshot> => makeSettings()),
    pickReceiveDir: vi.fn(async () => null),
    setAutoClipboard: vi.fn(async (): Promise<SettingsSnapshot> => makeSettings()),
    setAutostart: vi.fn(async (): Promise<SettingsSnapshot> => makeSettings()),
    setNetworkSelection: vi.fn(async (): Promise<NetworkStatusView> => makeNetwork()),
    clearDrop: vi.fn(),
    respondTransfer: vi.fn(async (): Promise<TransferSummary> => makeTransfer()),
    controlTransfer: vi.fn(async (): Promise<TransferSummary> => makeTransfer()),
    clearTransferError: vi.fn(),
    clearPairingNotice: vi.fn(),
  };
  return { ...base, ...options };
}

export function MockEngineProvider(props: ParentProps<{ engine: EngineContextValue }>) {
  return <EngineContext.Provider value={props.engine}>{props.children}</EngineContext.Provider>;
}
