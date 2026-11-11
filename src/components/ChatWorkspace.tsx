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
  Link2,
  MessageCircle,
  Paperclip,
  Send,
  ShieldCheck,
  Wifi,
  WifiOff,
} from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, Textarea, toast } from "../ui";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuPortal,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "../ui/context-menu";
import { Bubble, BubbleContent } from "../ui/bubble";
import { Message, MessageContent } from "../ui/message";
import {
  Attachment,
  AttachmentContent,
  AttachmentDescription,
  AttachmentMedia,
  AttachmentTitle,
} from "../ui/attachment";
import {
  MessageScroller,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from "../ui/message-scroller";
import type { ChatMessageView, TransferSummary } from "../bindings";
import { formatBytes, formatClock, formatSpeed } from "../lib/format";
import { fileKind } from "../lib/file-kind";
import { failureReason } from "../lib/transfer-errors";
import { TransferActions } from "./TransferActions";

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
    case "queued":
      return t("transfers.state.queued");
    case "preparing":
      return t("transfers.state.preparing");
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
/**
 * The library names its attachment states for the sender's side of an
 * upload; this project names them for both directions.
 */
function attachmentState(state: string) {
  if (state === "transferring") return "uploading" as const;
  if (state === "failed") return "error" as const;
  return "done" as const;
}

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
  // The card's menu does its work outside the timeline, so the dialogs it
  // opens are held here rather than inside the message list.
  const [detailsOf, setDetailsOf] = createSignal<TransferSummary | null>(null);
  const [deleteOf, setDeleteOf] = createSignal<TransferSummary | null>(null);
  const [forwardOf, setForwardOf] = createSignal<TransferSummary | null>(null);
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
  /// Devices a finished file can be forwarded to.
  ///
  /// The peer it already reached is left out: sending it back is a plain
  /// re-send of the same file, which is a different intention from forwarding
  /// a copy somewhere else.
  const forwardTargets = (transfer: TransferSummary) =>
    (engine.snapshot()?.peers ?? []).filter(
      (peer) => peer.online && peer.trusted && peer.device_id !== transfer.device_id,
    );

  const describe = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

  /// Hand a finished receive to the platform's default app.
  ///
  /// The call used to be `void`ed, so a refusal from the platform - the file
  /// is gone, nothing is registered for the type, the installer will not run -
  /// looked exactly like a card that does nothing when clicked.
  async function openReceived(transfer: TransferSummary) {
    try {
      await engine.openReceivedTransfer(transfer.transfer_id);
    } catch (cause) {
      toast.error(t("chat.openFailed", { message: describe(cause) }));
    }
  }

  async function revealFile(transfer: TransferSummary) {
    try {
      await engine.revealTransfer(transfer.transfer_id);
    } catch (cause) {
      toast.error(t("transfers.revealFailed", { message: describe(cause) }));
    }
  }

  async function deleteFile(transfer: TransferSummary) {
    try {
      await engine.deleteTransferFile(transfer.transfer_id);
      toast.success(t("transfers.deleted", { name: transfer.file_name }));
    } catch (cause) {
      toast.error(t("transfers.deleteFailed", { message: describe(cause) }));
    } finally {
      setDeleteOf(null);
    }
  }

  async function forwardFile(transfer: TransferSummary, deviceId: string) {
    try {
      await engine.forwardTransfer(transfer.transfer_id, deviceId);
      toast.success(t("transfers.forwardStarted", { name: transfer.file_name }));
    } catch (cause) {
      toast.error(t("transfers.forwardFailed", { message: describe(cause) }));
    } finally {
      setForwardOf(null);
    }
  }

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
  const canSend = () => Boolean(selected()?.online) && !sending() && draft().trim().length > 0;

  onMount(() => {
    const query = window.matchMedia("(max-width: 768px)");
    setCompact(query.matches);
    const listener = (event: MediaQueryListEvent) => setCompact(event.matches);
    query.addEventListener("change", listener);
    onCleanup(() => query.removeEventListener("change", listener));
    // Only a list with nothing open picks a thread for the user. A route
    // that already names a device is an instruction, not a default: opening
    // the first conversation here used to replace it and send the deep link
    // to whoever happened to be at the top of the list.
    if (compact() || selectedId()) return;
    const first = conversations()[0];
    if (first) openConversation(first.device_id);
  });

  createEffect(
    on(
      () =>
        conversations()
          .map((item) => item.device_id)
          .join("|"),
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
      pinnedToBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 48;
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
      if (onAndroid) {
        // The platform picker hands back one descriptor, so a phone sends one
        // file per tap; the desktop dialog can return a whole selection.
        const file = await engine.pickSourceFile();
        if (!file) return;
        await engine.startTransfers(peer.device_id, [file.file_id]);
        toast.success(t("chat.attachSent", { name: file.name }));
        return;
      }
      const files = await engine.pickFiles(t("chat.attachTitle"));
      if (files.length === 0) return;
      await engine.startTransfers(
        peer.device_id,
        files.map((file) => file.file_id),
      );
      toast.success(
        files.length === 1
          ? t("chat.attachSent", { name: files[0].name })
          : t("chat.attachSentMany", { count: files.length }),
      );
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause));
    }
  }

  // Files dropped on the window are handed to the conversation on screen. The
  // engine keeps the last drop until something claims it, so a drop that lands
  // while the chat is closed is still there when it opens.
  createEffect(() => {
    const dropped = engine.drop();
    const peer = selected();
    if (!dropped || !peer) return;
    engine.clearDrop();
    const fileIds = dropped.files.map((file) => file.file_id);
    const folderIds = dropped.folders.map((folder) => folder.folder_id);
    if (fileIds.length === 0 && folderIds.length === 0) return;
    void (async () => {
      try {
        if (fileIds.length > 0) await engine.startTransfers(peer.device_id, fileIds);
        for (const folderId of folderIds) await engine.startFolder(peer.device_id, folderId);
        const count = fileIds.length + folderIds.length;
        toast.success(
          count === 1
            ? t("chat.attachSent", {
                name: dropped.files[0]?.name ?? dropped.folders[0]?.root_name ?? "",
              })
            : t("chat.attachSentMany", { count }),
        );
      } catch (cause) {
        toast.error(cause instanceof Error ? cause.message : String(cause));
      }
    })();
  });

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
    <>
      <section
        class="relative grid min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-card"
        classList={{
          "grid-cols-[minmax(15rem,18.75rem)_minmax(0,1fr)]": !compact(),
        }}
        aria-label={t("chat.title")}
      >
        <Show when={!compact() || !selected()}>
          <aside
            class="flex min-h-0 flex-col border-border bg-muted/30"
            classList={{ "border-r": !compact() }}
            aria-label={t("chat.listAria")}
          >
            <header class="flex items-center justify-between gap-2.5 px-4 pt-3.5 pb-2.5">
              <div class="flex min-w-0 flex-col gap-0.5">
                <h2 class="text-title font-[620] tracking-[-0.012em]">{t("chat.title")}</h2>
                <p class="text-body-sm text-muted-foreground">{t("chat.memoryNote")}</p>
              </div>
              <span class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                {conversations().length}
              </span>
            </header>
            <Show
              when={conversations().length > 0}
              fallback={
                <div class="flex flex-col items-center gap-1 p-4 text-center text-muted-foreground">
                  <MessageCircle size={18} aria-hidden="true" />
                  <p class="text-body font-medium">{t("chat.noConversations")}</p>
                  <span class="text-label">{t("chat.noConversationsBody")}</span>
                </div>
              }
            >
              <ul class="flex min-h-0 flex-1 flex-col overflow-y-auto pb-2">
                <For each={conversations()}>
                  {(peer) => (
                    <li>
                      <button
                        type="button"
                        class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left transition-colors hover:bg-muted aria-[current=page]:bg-accent"
                        aria-current={selected()?.device_id === peer.device_id ? "true" : undefined}
                        onClick={() => openConversation(peer.device_id)}
                      >
                        <span
                          class="size-2 shrink-0 rounded-full"
                          classList={{
                            "bg-success": peer.online,
                            "bg-warning": !peer.online && peer.session_state === "connecting",
                            "bg-muted-foreground/40":
                              !peer.online && peer.session_state !== "connecting",
                          }}
                          aria-hidden="true"
                        />
                        <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                          <strong class="text-body font-semibold">
                            {displayName(peer.device_id, peer.name)}
                          </strong>
                          <span
                            class="text-label text-muted-foreground"
                            classList={{ "font-mono": lastPreview(peer.device_id).isFile }}
                          >
                            {lastPreview(peer.device_id).label ?? t("chat.noMessages")}
                          </span>
                        </span>
                        <span class="flex shrink-0 flex-col items-end gap-0.5 text-label text-muted-foreground">
                          <Show when={lastPreview(peer.device_id).at > 0}>
                            <time class="font-mono text-label">
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
        </Show>

        <Show when={!compact() || selected()}>
          <Show
            when={selected()}
            fallback={
              <div class="flex flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
                <MessageCircle size={26} aria-hidden="true" />
                <h2 class="text-title font-[620] tracking-[-0.012em]">{t("chat.selectTitle")}</h2>
                <p class="text-body-sm text-muted-foreground">{t("chat.selectBody")}</p>
              </div>
            }
          >
            {(peer) => (
              <div class="flex min-h-0 flex-col">
                <header class="flex items-center gap-3 border-b border-border px-4 py-3">
                  <Show when={compact()}>
                    <button
                      type="button"
                      class="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                      aria-label={t("chat.back")}
                      onClick={() => closeConversation()}
                    >
                      <ChevronLeft size={18} aria-hidden="true" />
                    </button>
                  </Show>
                  <div>
                    <h2 class="text-heading font-[620] tracking-[-0.015em]">
                      {displayName(peer().device_id, peer().name)}
                    </h2>
                    <p class="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <Show
                        when={peer().online}
                        fallback={<WifiOff size={12} aria-hidden="true" />}
                      >
                        <Show
                          when={peer().session_state === "connected"}
                          fallback={<Wifi size={12} aria-hidden="true" />}
                        >
                          <Link2 size={12} aria-hidden="true" />
                        </Show>
                      </Show>
                      <span>
                        {peer().online
                          ? peer().session_state === "connected"
                            ? t("chat.connected")
                            : t("chat.online")
                          : t("chat.offline")}
                      </span>
                    </p>
                  </div>
                  <span class="rounded-full border border-border px-2 py-0.5 text-label text-muted-foreground">
                    {t("chat.memoryPill")}
                  </span>
                </header>

                <MessageScrollerProvider autoScroll>
                  <MessageScroller class="min-h-0 flex-1">
                    <MessageScrollerViewport aria-label={t("chat.listAria")}>
                      <MessageScrollerContent class="gap-2 p-4">
                        <Show
                          when={selectedTimeline().length > 0}
                          fallback={
                            <div class="m-auto flex flex-col items-center gap-2 text-center text-muted-foreground">
                              <MessageCircle size={24} aria-hidden="true" />
                              <p class="text-body font-medium">{t("chat.noMessages")}</p>
                              <span>{t("chat.noMessagesBody")}</span>
                            </div>
                          }
                        >
                          <For
                            each={selectedTimeline().map((entry) =>
                              entry.kind === "file"
                                ? "file:" + entry.transfer.transfer_id
                                : "text:" + entry.message.message_id,
                            )}
                          >
                            {(key) => {
                              const isFile = key.startsWith("file:");
                              // The row is keyed by id, so it survives a progress tick.
                              // The data is read through the key every render instead of
                              // being captured once, which is what kept remounting it.
                              const file = () =>
                                selectedTimeline().find(
                                  (candidate) =>
                                    candidate.kind === "file" &&
                                    "file:" + candidate.transfer.transfer_id === key,
                                ) as Extract<TimelineEntry, { kind: "file" }> | undefined;
                              const text = () =>
                                selectedTimeline().find(
                                  (candidate) =>
                                    candidate.kind === "text" &&
                                    "text:" + candidate.message.message_id === key,
                                ) as Extract<TimelineEntry, { kind: "text" }> | undefined;
                              if (isFile) {
                                const transfer = () => file()!.transfer;
                                // A batch walks through its files one at a time, so each new
                                // file re-enters `preparing` while the cumulative byte counters
                                // keep climbing. Both states are "moving".
                                const moving = () =>
                                  transfer().state === "transferring" ||
                                  transfer().state === "preparing";
                                const openable =
                                  transfer().direction === "receive" &&
                                  transfer().state === "completed";
                                const percent = () =>
                                  transfer().bytes_total > 0
                                    ? Math.round(
                                        (transfer().bytes_done / transfer().bytes_total) * 100,
                                      )
                                    : 0;
                                return (
                                  <MessageScrollerItem messageId={key}>
                                    <Message
                                      align={transfer().direction === "send" ? "end" : "start"}
                                    >
                                      <MessageContent>
                                        <ContextMenu
                                          disabled={
                                            !engine.desktop || transfer().state !== "completed"
                                          }
                                        >
                                          <ContextMenuTrigger class="contents">
                                            <Attachment
                                              state={attachmentState(transfer().state)}
                                              class="max-w-[min(22rem,80%)] cursor-pointer"
                                              // Only a received, finished file is worth opening: the
                                              // sender's copy is already theirs, and a file still
                                              // moving cannot be handed to another app.
                                              role={openable ? "button" : undefined}
                                              tabindex={openable ? 0 : undefined}
                                              onClick={() => {
                                                if (openable) void openReceived(transfer());
                                              }}
                                              onKeyDown={(event) => {
                                                if (!openable) return;
                                                if (event.key !== "Enter" && event.key !== " ")
                                                  return;
                                                event.preventDefault();
                                                void openReceived(transfer());
                                              }}
                                            >
                                              <AttachmentMedia>
                                                {(() => {
                                                  const Icon = fileKind(transfer().file_name).icon;
                                                  return <Icon size={18} aria-hidden="true" />;
                                                })()}
                                              </AttachmentMedia>
                                              <AttachmentContent>
                                                <AttachmentTitle>
                                                  {transfer().file_name}
                                                </AttachmentTitle>
                                                <AttachmentDescription>
                                                  {formatBytes(transfer().bytes_total)} ·{" "}
                                                  {transferLabel(transfer(), t)}
                                                  <span classList={{ invisible: !moving() }}>
                                                    {" · "}
                                                    {percent()}%
                                                    <Show when={transfer().bytes_per_second > 0}>
                                                      {" · "}
                                                      {formatSpeed(transfer().bytes_per_second)}
                                                    </Show>
                                                  </span>
                                                </AttachmentDescription>
                                                {/* The track stays mounted and only its ink hides:
                                  unmounting it mid-batch collapses the card and
                                  makes the scrollbar jump. */}
                                                <span
                                                  class="mt-1 block h-1 w-full overflow-hidden rounded-full bg-muted"
                                                  classList={{ invisible: !moving() }}
                                                  aria-hidden={!moving()}
                                                >
                                                  <span
                                                    class="block h-full rounded-full bg-primary"
                                                    style={{ width: `${percent()}%` }}
                                                  />
                                                </span>
                                                <Show when={transfer().state === "failed"}>
                                                  <span class="text-label text-destructive">
                                                    {failureReason(
                                                      transfer().error_code,
                                                      locale(),
                                                      transfer().error_message ??
                                                        t("chat.errorGeneric"),
                                                    )}
                                                  </span>
                                                </Show>
                                              </AttachmentContent>
                                            </Attachment>
                                          </ContextMenuTrigger>
                                          <ContextMenuPortal>
                                            <ContextMenuContent>
                                              <ContextMenuItem
                                                onClick={() => void revealFile(transfer())}
                                              >
                                                {t("transfers.menuReveal")}
                                              </ContextMenuItem>
                                              <ContextMenuItem
                                                variant="destructive"
                                                onClick={() => setDeleteOf(transfer())}
                                              >
                                                {t("transfers.menuDelete")}
                                              </ContextMenuItem>
                                              <ContextMenuSeparator />
                                              <ContextMenuItem
                                                onClick={() => setDetailsOf(transfer())}
                                              >
                                                {t("transfers.menuDetails")}
                                              </ContextMenuItem>
                                              <ContextMenuItem
                                                disabled={forwardTargets(transfer()).length === 0}
                                                onClick={() => setForwardOf(transfer())}
                                              >
                                                {t("transfers.menuForward")}
                                              </ContextMenuItem>
                                            </ContextMenuContent>
                                          </ContextMenuPortal>
                                        </ContextMenu>
                                        <div
                                          class="flex flex-wrap items-center gap-2 text-label text-muted-foreground"
                                          classList={{
                                            "self-end": transfer().direction === "send",
                                            "flex-row-reverse": transfer().direction === "send",
                                          }}
                                        >
                                          <time class="font-mono text-label">
                                            {formatClock(transfer().created_at_ms, locale())}
                                          </time>
                                          <TransferActions
                                            transfer={transfer()}
                                            busy={engine.transferBusy() === transfer().transfer_id}
                                            onAction={(action) => {
                                              void engine
                                                .controlTransfer(transfer().transfer_id, action)
                                                .catch((cause: unknown) => {
                                                  toast.error(
                                                    cause instanceof Error
                                                      ? cause.message
                                                      : String(cause),
                                                  );
                                                });
                                            }}
                                          />
                                        </div>
                                      </MessageContent>
                                    </Message>
                                  </MessageScrollerItem>
                                );
                              }

                              const message = () => text()!.message;
                              const sent = message().direction === "sent";
                              return (
                                <MessageScrollerItem messageId={key}>
                                  <Message align={sent ? "end" : "start"}>
                                    <MessageContent>
                                      <Bubble variant={sent ? "default" : "muted"}>
                                        <BubbleContent class="px-3 py-2 text-sm break-words">
                                          {message().text}
                                        </BubbleContent>
                                      </Bubble>
                                      <div
                                        class="flex flex-wrap items-center gap-2 text-label text-muted-foreground"
                                        classList={{ "self-end": sent, "flex-row-reverse": sent }}
                                      >
                                        <time class="font-mono text-label">
                                          {formatClock(message().created_at_ms, locale())}
                                        </time>
                                        <span
                                          classList={{
                                            "font-semibold": message().state === "failed",
                                          }}
                                        >
                                          {statusLabel(message())}
                                        </span>
                                        <Show
                                          when={message().state === "failed" && message().retryable}
                                        >
                                          <button
                                            type="button"
                                            class="text-xs text-primary underline-offset-2 hover:underline"
                                            disabled={retrying() !== null || !peer().online}
                                            title={
                                              !peer().online ? t("chat.offlineHint") : undefined
                                            }
                                            aria-label={t("chat.retryAria", {
                                              name: displayName(peer().device_id, peer().name),
                                            })}
                                            onClick={() => void retry(message())}
                                          >
                                            {retrying() === message().message_id
                                              ? t("chat.retrying")
                                              : t("chat.retry")}
                                          </button>
                                        </Show>
                                        <Show
                                          when={
                                            message().state === "failed" && message().error_message
                                          }
                                        >
                                          <p class="text-label text-destructive">
                                            {errorLabel(message())}
                                          </p>
                                        </Show>
                                      </div>
                                    </MessageContent>
                                  </Message>
                                </MessageScrollerItem>
                              );
                            }}
                          </For>
                        </Show>
                      </MessageScrollerContent>
                    </MessageScrollerViewport>
                  </MessageScroller>
                </MessageScrollerProvider>

                <Show
                  when={peer().online}
                  fallback={
                    <div class="mx-4 mb-2 rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">
                      <WifiOff size={16} aria-hidden="true" />
                      <span>{t("chat.offlineHint")}</span>
                    </div>
                  }
                >
                  <div class="border-t border-border p-3">
                    <Textarea
                      value={draft()}
                      onInput={(event: InputEvent & { currentTarget: HTMLTextAreaElement }) =>
                        setDraft(event.currentTarget.value)
                      }
                      onKeyDown={onComposerKeyDown}
                      placeholder={t("chat.placeholder")}
                      aria-label={t("chat.placeholder")}
                      rows={2}
                      disabled={sending()}
                    />
                    <div class="mt-2 flex items-center justify-between gap-3">
                      <span class="text-label text-muted-foreground">{t("chat.enterHint")}</span>
                      <div class="flex items-center gap-2">
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
        </Show>
        {/* The OS owns the drag; this is only the invitation to drop. */}
        <Show when={engine.dragActive() && selected()}>
          <div class="pointer-events-none absolute inset-0 z-10 grid place-items-center rounded-lg border-2 border-dashed border-primary/60 bg-background/80">
            <div class="flex flex-col items-center gap-1.5">
              <Paperclip size={22} class="text-primary" aria-hidden="true" />
              <p class="text-body font-medium">{t("chat.dropHere")}</p>
            </div>
          </div>
        </Show>
      </section>

      {/* Details, delete and forward live outside the timeline so the list
          keeps its own scroll position while a dialog is open. */}
      <Dialog
        open={detailsOf() !== null}
        onOpenChange={(open) => {
          if (!open) setDetailsOf(null);
        }}
        title={t("transfers.detailsTitle")}
      >
        <Show when={detailsOf()}>
          {(transfer) => (
            <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-body-sm">
              <dt class="text-muted-foreground">{t("transfers.detailsFile")}</dt>
              <dd class="break-all">{transfer().file_name}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsSize")}</dt>
              <dd class="font-mono">{formatBytes(transfer().bytes_total)}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsDirection")}</dt>
              <dd>
                {transfer().direction === "send"
                  ? t("transfers.sendDirection")
                  : t("transfers.receiveDirection")}
              </dd>
              <dt class="text-muted-foreground">{t("transfers.detailsPeer")}</dt>
              <dd>{displayName(transfer().device_id, transfer().device_name)}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsState")}</dt>
              <dd>{transferLabel(transfer(), t)}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsCreated")}</dt>
              <dd class="font-mono">{formatClock(transfer().created_at_ms, locale())}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsUpdated")}</dt>
              <dd class="font-mono">{formatClock(transfer().updated_at_ms, locale())}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsTransferId")}</dt>
              <dd class="font-mono text-xs break-all">{transfer().transfer_id}</dd>
            </dl>
          )}
        </Show>
      </Dialog>

      <Dialog
        open={deleteOf() !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteOf(null);
        }}
        title={t("transfers.deleteTitle")}
        description={t("transfers.deleteBody", {
          name: deleteOf()?.file_name ?? "",
        })}
        footer={
          <>
            <Button variant="ghost" onClick={() => setDeleteOf(null)}>
              {t("transfers.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                const target = deleteOf();
                if (target) void deleteFile(target);
              }}
            >
              {t("transfers.deleteConfirm")}
            </Button>
          </>
        }
      />

      <Dialog
        open={forwardOf() !== null}
        onOpenChange={(open) => {
          if (!open) setForwardOf(null);
        }}
        title={t("transfers.forwardTitle")}
        description={t("transfers.forwardBody", {
          name: forwardOf()?.file_name ?? "",
        })}
      >
        <Show
          when={forwardOf()}
          fallback={<p class="text-body-sm text-muted-foreground">{t("transfers.forwardEmpty")}</p>}
        >
          {(transfer) => (
            <ul class="flex flex-col gap-1">
              <For each={forwardTargets(transfer())}>
                {(peer) => (
                  <li>
                    <Button
                      variant="ghost"
                      class="w-full justify-start"
                      onClick={() => void forwardFile(transfer(), peer.device_id)}
                    >
                      {displayName(peer.device_id, peer.name)}
                    </Button>
                  </li>
                )}
              </For>
            </ul>
          )}
        </Show>
      </Dialog>
    </>
  );
}
