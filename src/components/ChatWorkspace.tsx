import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  on,
  onCleanup,
  onMount,
} from "solid-js";
import {
  ChevronLeft,
  MessageCircle,
  Paperclip,
  Send,
  ShieldCheck,
  WifiOff,
} from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Textarea, toast } from "../ui";
import type { ChatMessageView, TransferSummary } from "../bindings";
import { formatBytes, formatClock } from "../lib/format";
import { fileKind } from "../lib/file-kind";
import { failureReason } from "../lib/transfer-errors";

/// Sending a file from the conversation instead of the transfers page: the two
/// belong together, and switching pages to attach something is the whole point of
/// the complaint this exists for.

type TimelineEntry =
  | { kind: "text"; at: number; message: ChatMessageView }
  | { kind: "file"; at: number; transfer: TransferSummary };

function transferLabel(transfer: TransferSummary, t: (key: string) => string): string {
  switch (transfer.state) {
    case "completed":
      return t("transfers.state.completed");
    case "failed":
      return t("transfers.state.failed");
    case "paused":
      return t("transfers.state.paused");
    case "interrupted":
      return t("transfers.state.interrupted");
    case "cancelled":
      return t("transfers.state.cancelled");
    case "offered":
      return t("transfers.state.offered");
    default:
      return t("transfers.state.transferring");
  }
}

function failureMessage(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  return String(cause);
}

/**
 * The open conversation is controlled by the caller rather than read from the
 * router. A route is how the app expresses "this thread is open" - it gives the
 * phone's back button somewhere to go - but the workspace itself stays a plain
 * component, which keeps it testable without a router around it.
 */
export function ChatWorkspace(props: {
  selectedId?: string;
  onSelect?: (deviceId: string | null) => void;
}) {
  const engine = useEngine();
  const { t, locale } = useI18n();
  const conversations = () => engine.chatConversations();
  const messages = () => engine.chatMessages();
  const selectedId = () => props.selectedId ?? null;
  const openConversation = (deviceId: string) => props.onSelect?.(deviceId);
  const closeConversation = () => props.onSelect?.(null);
  const [draft, setDraft] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [retrying, setRetrying] = createSignal<string | null>(null);
  // A phone-shaped viewport cannot show a conversation list and a thread side by
  // side: 218px of list squeezed the messages into a letterbox. Compact mode
  // swaps one for the other, the way a chat app does, instead of stacking two
  // scroll areas inside a square screen.
  const [compact, setCompact] = createSignal(false);
  let messageList: HTMLDivElement | undefined;

  const selected = createMemo(() => {
    const list = conversations();
    const id = selectedId();
    if (!id) {
      // Desktop keeps a thread open by default; a compact screen starts on the
      // list so the user picks who to talk to.
      return compact() ? undefined : list[0];
    }
    return list.find((item) => item.device_id === id) ?? (compact() ? undefined : list[0]);
  });
  // The conversation carries the name recorded by the pairing handshake, so a
  // peer that renamed itself afterwards keeps the old label here while the
  // trusted-device list - which reads the live snapshot - is already correct.
  // Prefer the snapshot's name whenever it has one for this device.
  const displayName = (deviceId: string, fallback: string): string => {
    const live = engine.snapshot()?.peers.find((peer) => peer.device_id === deviceId);
    return live?.name ?? fallback;
  };
  const selectedMessages = createMemo(() => {
    const peer = selected();
    if (!peer) return [];
    return messages().filter((message) => message.device_id === peer.device_id);
  });
  /**
   * Text and files in one stream, ordered by when they started.
   *
   * The two used to live on separate pages, which meant a conversation was
   * something you had to piece together from two screens. A message and the
   * file you sent with it are one exchange, so they belong in one list.
   */
  const selectedTimeline = createMemo(() => {
    const peer = selected();
    if (!peer) return [] as TimelineEntry[];
    const texts: TimelineEntry[] = selectedMessages().map((message) => ({
      kind: "text" as const,
      at: message.created_at_ms,
      message,
    }));
    const files: TimelineEntry[] = engine
      .transfers()
      .filter((item) => item.device_id === peer.device_id)
      .map((transfer) => ({
        kind: "file" as const,
        at: transfer.created_at_ms,
        transfer,
      }));
    return [...texts, ...files].sort((a, b) => a.at - b.at);
  });
  const canSend = () =>
    Boolean(selected()?.online) && !sending() && draft().trim().length > 0;

  onMount(() => {
    const query = window.matchMedia("(max-width: 768px)");
    setCompact(query.matches);
    const listener = (event: MediaQueryListEvent) => setCompact(event.matches);
    query.addEventListener("change", listener);
    onCleanup(() => query.removeEventListener("change", listener));
    if (compact()) return;
    const first = conversations()[0];
    if (first) openConversation(first.device_id);
  });

  createEffect(
    on(
      () => conversations().map((item) => item.device_id).join("|"),
      () => {
        const current = selectedId();
        if (current && !conversations().some((item) => item.device_id === current)) {
          closeConversation();
        }
      },
    ),
  );

  createEffect(
    on(
      () => {
        const peer = selected();
        return peer ? `${peer.device_id}:${peer.online}` : "";
      },
      () => {
        const peer = selected();
        if (!peer?.online) return;
        void engine.openChat(peer.device_id).catch(() => undefined);
      },
      { defer: true },
    ),
  );

  // Drill down to primitive values: `selected()` returns a fresh object on
  // every snapshot, so depending on it would re-run this on each poll and haul
  // the list back to the bottom while someone is reading history.
  const selectedDeviceId = createMemo(() => selected()?.device_id);
  const selectedMessageCount = createMemo(() => selectedMessages().length);
  // Whether the view is following the tail, updated from the scroll event so
  // the decision is made against the pre-append layout.
  let pinnedToBottom = true;
  let lastDeviceId: string | undefined;
  let lastCount = 0;

  createEffect(() => {
    const deviceId = selectedDeviceId();
    const count = selectedMessageCount();
    const switched = deviceId !== lastDeviceId;
    const grew = count > lastCount;
    lastDeviceId = deviceId;
    lastCount = count;
    if (!switched && !grew) return;
    if (!switched && !pinnedToBottom) return;
    queueMicrotask(() => {
      if (!messageList) return;
      messageList.scrollTop = messageList.scrollHeight;
      pinnedToBottom = true;
    });
  });

  onMount(() => {
    const list = messageList;
    if (!list) return;
    const onScroll = () => {
      // Within a line or two of the tail counts as "following": the last
      // message can be taller than the viewport, and an exact comparison
      // would unpin on a fractional scroll position.
      pinnedToBottom =
        list.scrollHeight - list.scrollTop - list.clientHeight < 48;
    };
    list.addEventListener("scroll", onScroll, { passive: true });
    onCleanup(() => list.removeEventListener("scroll", onScroll));
  });

  async function attach() {
    const peer = selected();
    if (!peer) return;
    try {
      // A phone has no file dialog the desktop UI can drive, so it uses the
      // platform picker and gets a descriptor back; the desktop opens its own.
      // The platform decides this, not the IPC bridge: both builds expose
      // __TAURI_INTERNALS__, so engine.desktop is true on Android too.
      const onAndroid = engine.snapshot()?.status.platform === "android";
      const file = onAndroid
        ? await engine.pickSourceFile()
        : await engine.pickFile(t("chat.attachTitle"));
      if (!file) return;
      await engine.startTransfers(peer.device_id, [file.file_id]);
      toast.success(t("chat.attachSent", { name: file.name }));
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function send() {
    const peer = selected();
    const text = draft().trim();
    if (!peer || !text || !peer.online || sending()) return;
    setSending(true);
    try {
      await engine.sendChat(peer.device_id, text);
      setDraft("");
    } catch (cause) {
      toast.error(t("chat.sendFailed", { message: failureMessage(cause) }));
    } finally {
      setSending(false);
    }
  }

  async function retry(message: ChatMessageView) {
    const peer = selected();
    if (!peer || retrying()) return;
    setRetrying(message.message_id);
    try {
      await engine.retryChat(peer.device_id, message.message_id);
    } catch (cause) {
      toast.error(t("chat.retryFailed", { message: failureMessage(cause) }));
    } finally {
      setRetrying(null);
    }
  }

  function onComposerKeyDown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  function errorLabel(message: ChatMessageView): string {
    switch (message.error_code) {
      case "connection_lost":
        return t("chat.errorConnectionLost");
      case "send_timeout":
        return t("chat.errorSendTimeout");
      case "not_delivered":
        return t("chat.errorNotDelivered");
      case "session_closed":
        return t("chat.errorSessionClosed");
      case "transport":
        return t("chat.errorTransport");
      default:
        return message.error_message ?? t("chat.errorGeneric");
    }
  }

  function statusLabel(message: ChatMessageView): string {
    if (message.direction === "received") return t("chat.received");
    switch (message.state) {
      case "sending":
        return t("chat.sending");
      case "delivered":
        return t("chat.delivered");
      case "failed":
        return t("chat.failed");
      default:
        return t("chat.received");
    }
  }

  /**
   * The last thing that happened with a peer, whether text or a file.
   *
   * A conversation list that only counts messages looks empty after a file
   * send, which is the one thing the other side is waiting on. Whichever
   * happened later wins.
   */
  const lastPreview = (deviceId: string) => {
    const peer = conversations().find((item) => item.device_id === deviceId);
    const textAt = peer?.last_at_ms ?? 0;
    const transfer = engine
      .transfers()
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
  };

  return (
    <section
      class="fx-chat"
      classList={{ "is-compact": compact(), "is-thread": compact() && selected() !== undefined }}
      aria-label={t("chat.title")}
    >
      <aside class="fx-chat-list" aria-label={t("chat.listAria")}>
        <header class="fx-chat-list-head">
          <div>
            <h2>{t("chat.title")}</h2>
            <p>{t("chat.memoryNote")}</p>
          </div>
          <span class="fx-count">{conversations().length}</span>
        </header>
        <Show
          when={conversations().length > 0}
          fallback={
            <div class="fx-chat-list-empty">
              <MessageCircle size={18} aria-hidden="true" />
              <p>{t("chat.noConversations")}</p>
              <span>{t("chat.noConversationsBody")}</span>
            </div>
          }
        >
          <ul class="fx-chat-conversations">
            <For each={conversations()}>
              {(peer) => (
                <li>
                  <button
                    type="button"
                    class="fx-chat-conversation"
                    classList={{ "is-active": selected()?.device_id === peer.device_id }}
                    aria-current={selected()?.device_id === peer.device_id ? "true" : undefined}
                    onClick={() => openConversation(peer.device_id)}
                  >
                    <span
                      class="fx-chat-presence"
                      classList={{
                        "is-online": peer.online,
                        "is-connecting": peer.session_state === "connecting",
                      }}
                      aria-hidden="true"
                    />
                    <span class="fx-chat-conversation-main">
                      <strong>{displayName(peer.device_id, peer.name)}</strong>
                      <span classList={{ "fx-chat-preview-file": lastPreview(peer.device_id).isFile }}>
                        {lastPreview(peer.device_id).label ?? t("chat.noMessages")}
                      </span>
                    </span>
                    <span class="fx-chat-conversation-meta">
                      <Show when={lastPreview(peer.device_id).at > 0}>
                        <time class="fx-numeric">
                          {formatClock(lastPreview(peer.device_id).at, locale())}
                        </time>
                      </Show>
                      <Show when={peer.online}>
                        <ShieldCheck size={12} aria-label={t("chat.online")} />
                      </Show>
                    </span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </aside>

      <Show
        when={selected()}
        fallback={
          <div class="fx-chat-pane fx-chat-placeholder">
            <MessageCircle size={26} aria-hidden="true" />
            <h2>{t("chat.selectTitle")}</h2>
            <p>{t("chat.selectBody")}</p>
          </div>
        }
      >
        {(peer) => (
          <div class="fx-chat-pane">
            <header class="fx-chat-head">
              <Show when={compact()}>
                <button
                  type="button"
                  class="fx-chat-back"
                  aria-label={t("chat.back")}
                  onClick={() => closeConversation()}
                >
                  <ChevronLeft size={18} aria-hidden="true" />
                </button>
              </Show>
              <div>
                <h2>{displayName(peer().device_id, peer().name)}</h2>
                <p class="fx-chat-peer-state">
                  <span
                    class="fx-chat-presence"
                    classList={{
                      "is-online": peer().online,
                      "is-connecting": peer().session_state === "connecting",
                    }}
                    aria-hidden="true"
                  />
                  <span>
                    {peer().online
                      ? peer().session_state === "connected"
                        ? t("chat.connected")
                        : t("chat.online")
                      : t("chat.offline")}
                  </span>
                </p>
              </div>
              <span class="fx-chat-memory-pill">{t("chat.memoryPill")}</span>
            </header>

            <div class="fx-chat-messages" ref={messageList} aria-live="polite">
              <Show
                when={selectedTimeline().length > 0}
                fallback={
                  <div class="fx-chat-empty">
                    <MessageCircle size={24} aria-hidden="true" />
                    <p>{t("chat.noMessages")}</p>
                    <span>{t("chat.noMessagesBody")}</span>
                  </div>
                }
              >
                <For each={selectedTimeline()}>
                  {(entry) => {
                    if (entry.kind === "file") {
                      const transfer = entry.transfer;
                      const percent = () =>
                        transfer.bytes_total > 0
                          ? Math.round((transfer.bytes_done / transfer.bytes_total) * 100)
                          : 0;
                      return (
                        <article
                          class="fx-chat-message"
                          classList={{ "is-sent": transfer.direction === "send" }}
                        >
                          <div
                            class="fx-chat-file"
                            classList={{
                              // Only a received, finished file is worth opening:
                              // the sender's copy is already theirs, and a file
                              // still moving cannot be handed to another app.
                              "is-openable":
                                transfer.direction === "receive" &&
                                transfer.state === "completed",
                            }}
                            onClick={() => {
                              if (
                                transfer.direction === "receive" &&
                                transfer.state === "completed"
                              ) {
                                void engine.openReceivedTransfer(transfer.transfer_id);
                              }
                            }}
                          >
                            <span
                              class={`fx-chat-file-icon is-${fileKind(transfer.file_name).tone}`}
                            >
                              {(() => {
                                const Icon = fileKind(transfer.file_name).icon;
                                return <Icon size={18} aria-hidden="true" />;
                              })()}
                            </span>
                            <div class="fx-chat-file-body">
                              <p class="fx-chat-file-name">{transfer.file_name}</p>
                              <span class="fx-chat-file-meta">
                                {formatBytes(transfer.bytes_total)} ·{" "}
                                {transferLabel(transfer, t)}
                                <Show when={transfer.state === "transferring"}>
                                  {" · "}
                                  {percent()}%
                                </Show>
                              </span>
                              <Show when={transfer.state === "transferring"}>
                                <span class="fx-chat-file-bar">
                                  <span style={{ width: `${percent()}%` }} />
                                </span>
                              </Show>
                              {/* A bare "failed" for every failure hides the one
                                  thing the user needs: whether the peer declined,
                                  the disk filled up, or the source moved. */}
                              <Show when={transfer.state === "failed"}>
                                <span class="fx-chat-file-reason">
                                  {failureReason(
                                    transfer.error_code,
                                    locale(),
                                    transfer.error_message ?? t("chat.errorGeneric"),
                                  )}
                                </span>
                              </Show>
                            </div>
                          </div>
                          <div class="fx-chat-message-meta">
                            <time class="fx-numeric">
                              {formatClock(transfer.created_at_ms, locale())}
                            </time>
                          </div>
                        </article>
                      );
                    }
                    const message = entry.message;
                    return (
                      <article
                        class="fx-chat-message"
                        classList={{ "is-sent": message.direction === "sent" }}
                      >
                        <div class="fx-chat-bubble">{message.text}</div>
                        <div class="fx-chat-message-meta">
                          <time class="fx-numeric">
                            {formatClock(message.created_at_ms, locale())}
                          </time>
                          <span classList={{ "is-failed": message.state === "failed" }}>
                            {statusLabel(message)}
                          </span>
                          <Show when={message.state === "failed" && message.retryable}>
                            <button
                              type="button"
                              class="fx-chat-retry"
                              disabled={retrying() !== null || !peer().online}
                              title={!peer().online ? t("chat.offlineHint") : undefined}
                              aria-label={t("chat.retryAria", {
                                name: displayName(peer().device_id, peer().name),
                              })}
                              onClick={() => void retry(message)}
                            >
                              {retrying() === message.message_id
                                ? t("chat.retrying")
                                : t("chat.retry")}
                            </button>
                          </Show>
                        </div>
                        <Show when={message.state === "failed" && message.error_message}>
                          <p class="fx-chat-error">{errorLabel(message)}</p>
                        </Show>
                      </article>
                    );
                  }}
                </For>
              </Show>
            </div>

            <Show
              when={peer().online}
              fallback={
                <div class="fx-chat-offline">
                  <WifiOff size={16} aria-hidden="true" />
                  <span>{t("chat.offlineHint")}</span>
                </div>
              }
            >
              <div class="fx-chat-composer">
                <Textarea
                  value={draft()}
                  onInput={(event) => setDraft(event.currentTarget.value)}
                  onKeyDown={onComposerKeyDown}
                  placeholder={t("chat.placeholder")}
                  aria-label={t("chat.placeholder")}
                  rows={2}
                  disabled={sending()}
                />
                <div class="fx-chat-composer-foot">
                  <span class="fx-chat-hint">{t("chat.enterHint")}</span>
                  <div class="fx-chat-actions">
                    <Button
                      size="sm"
                      variant="outline"
                      aria-label={t("chat.attach")}
                      title={t("chat.attach")}
                      onClick={() => void attach()}
                    >
                      <Paperclip size={15} aria-hidden="true" />
                      {t("chat.attach")}
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={!canSend()}
                      onClick={() => void send()}
                    >
                      <Send size={15} aria-hidden="true" />
                      {sending() ? t("chat.sending") : t("chat.send")}
                    </Button>
                  </div>
                </div>
              </div>
            </Show>
          </div>
        )}
      </Show>
    </section>
  );
}

