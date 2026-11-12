import type {
  ChatConversationView,
  ChatMessageView,
  PeerSummary,
  TransferSummary,
} from "../../bindings";
import type { useI18n } from "../../i18n";
import type { TimelineEntry } from "./ChatWorkspace.types";

type Translator = ReturnType<typeof useI18n>["t"];

/**
 * Text and files in one stream, ordered by when they started.
 *
 * The two used to live on separate pages, which meant a conversation was
 * something you had to piece together from two screens. A message and the
 * file you sent with it are one exchange, so they belong in one list.
 */
export function buildTimeline(
  messages: ChatMessageView[],
  transfers: TransferSummary[],
  deviceId: string,
): TimelineEntry[] {
  const texts: TimelineEntry[] = messages
    .filter((message) => message.device_id === deviceId)
    .map((message) => ({ kind: "text" as const, at: message.created_at_ms, message }));
  const files: TimelineEntry[] = transfers
    .filter((transfer) => transfer.device_id === deviceId)
    .map((transfer) => ({ kind: "file" as const, at: transfer.created_at_ms, transfer }));
  return [...texts, ...files].sort((a, b) => a.at - b.at);
}

/**
 * The last thing that happened with a peer, whether text or a file.
 *
 * A conversation list that only counts messages looks empty after a file send,
 * which is the one thing the other side is waiting on. Whichever happened
 * later wins.
 */
export function previewOf(
  conversations: ChatConversationView[],
  transfers: TransferSummary[],
  deviceId: string,
  t: Translator,
) {
  const peer = conversations.find((item) => item.device_id === deviceId);
  const textAt = peer?.last_at_ms ?? 0;
  const transfer = transfers
    .filter((item) => item.device_id === deviceId)
    .sort((a, b) => b.created_at_ms - a.created_at_ms)[0];
  if (transfer && transfer.created_at_ms > textAt) {
    return {
      label: t("chat.lastFile", { name: transfer.file_name }),
      at: transfer.created_at_ms,
      isFile: true,
    };
  }
  return { label: peer?.last_message ?? null, at: textAt, isFile: false };
}

/**
 * The live name of a peer, falling back to the one recorded at pairing.
 *
 * A conversation carries the name from the handshake, so a peer that renamed
 * itself afterwards keeps the old label here while the trusted-device list -
 * which reads the snapshot - is already correct.
 */
export function nameOf(peers: PeerSummary[], deviceId: string, fallback: string): string {
  const live = peers.find((peer) => peer.device_id === deviceId);
  return live?.name ?? fallback;
}

/// Every device a text can be forwarded to, including the one it came from:
/// quoting a message back to its author is a normal thing to want.
export function trustedPeers(peers: PeerSummary[]): PeerSummary[] {
  return peers.filter((peer) => peer.online && peer.trusted);
}

/// Devices a finished file can be forwarded to. The peer it already reached
/// is left out: sending it back is a plain re-send of the same file, which is
/// a different intention from forwarding a copy somewhere else.
export function forwardTargets(peers: PeerSummary[], transfer: TransferSummary): PeerSummary[] {
  return peers.filter(
    (peer) => peer.online && peer.trusted && peer.device_id !== transfer.device_id,
  );
}
