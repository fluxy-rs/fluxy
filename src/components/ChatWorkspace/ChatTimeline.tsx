import { For, Show, createMemo } from "solid-js";
import { MessageCircle } from "lucide-solid";
import type { ChatMessageView, TransferAction, TransferSummary } from "../../bindings";
import { useI18n } from "../../i18n";
import {
  MessageScroller,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from "../../ui/message-scroller";
import { FileCard } from "./FileCard";
import { TextBubble } from "./TextBubble";
import type { TimelineEntry } from "./ChatWorkspace.types";
import { useFollowTail } from "./useFollowTail";

/**
 * The conversation itself: text and files in one stream, oldest first.
 *
 * Rows are keyed by id so a progress tick updates a card instead of
 * remounting it - a remount used to reset the scroll position mid-transfer.
 */
export function ChatTimeline(props: {
  deviceId: string | undefined;
  entries: TimelineEntry[];
  busyTransferId: string | null;
  retryingMessageId: string | null;
  selectableMessageId: string | null;
  peerName: string;
  peerOnline: boolean;
  desktopMenuDisabled: boolean;
  canForward: (transfer: TransferSummary) => boolean;
  onOpenFile: (transfer: TransferSummary) => void;
  onRevealFile: (transfer: TransferSummary) => void;
  onDeleteFile: (transfer: TransferSummary) => void;
  onDetailsFile: (transfer: TransferSummary) => void;
  onForwardFile: (transfer: TransferSummary) => void;
  onFileLongPress: (event: PointerEvent, transfer: TransferSummary) => void;
  onTextLongPress: (event: PointerEvent, message: ChatMessageView) => void;
  onControl: (transfer: TransferSummary, action: TransferAction) => void;
  onRetry: (message: ChatMessageView) => void;
}) {
  const { t } = useI18n();
  let viewport: HTMLDivElement | undefined;

  const keys = createMemo(() =>
    props.entries.map((entry) =>
      entry.kind === "file"
        ? "file:" + entry.transfer.transfer_id
        : "text:" + entry.message.message_id,
    ),
  );

  useFollowTail({
    deviceId: () => props.deviceId,
    count: () => props.entries.length,
    viewport: () => viewport,
  });

  return (
    <MessageScrollerProvider autoScroll>
      <MessageScroller class="min-h-0 flex-1">
        <MessageScrollerViewport aria-label={t("chat.listAria")} ref={viewport}>
          <MessageScrollerContent class="gap-2 p-4">
            <Show
              when={props.entries.length > 0}
              fallback={
                <div class="m-auto flex flex-col items-center gap-2 text-center text-muted-foreground">
                  <MessageCircle size={24} aria-hidden="true" />
                  <p class="text-body font-medium">{t("chat.noMessages")}</p>
                  <span>{t("chat.noMessagesBody")}</span>
                </div>
              }
            >
              <For each={keys()}>
                {(key) => {
                  const file = () =>
                    props.entries.find(
                      (entry) =>
                        entry.kind === "file" && "file:" + entry.transfer.transfer_id === key,
                    ) as Extract<TimelineEntry, { kind: "file" }> | undefined;
                  const text = () =>
                    props.entries.find(
                      (entry) =>
                        entry.kind === "text" && "text:" + entry.message.message_id === key,
                    ) as Extract<TimelineEntry, { kind: "text" }> | undefined;
                  return (
                    <MessageScrollerItem messageId={key}>
                      <Show
                        when={file()}
                        fallback={
                          <Show when={text()}>
                            {(entry) => (
                              <TextBubble
                                message={entry().message}
                                peerName={props.peerName}
                                peerOnline={props.peerOnline}
                                retrying={props.retryingMessageId === entry().message.message_id}
                                selectable={
                                  props.selectableMessageId === entry().message.message_id
                                }
                                onLongPress={(event) =>
                                  props.onTextLongPress(event, entry().message)
                                }
                                onRetry={() => props.onRetry(entry().message)}
                              />
                            )}
                          </Show>
                        }
                      >
                        {(entry) => (
                          <FileCard
                            transfer={entry().transfer}
                            busy={props.busyTransferId === entry().transfer.transfer_id}
                            canForward={props.canForward(entry().transfer)}
                            desktopMenuDisabled={props.desktopMenuDisabled}
                            onOpen={() => props.onOpenFile(entry().transfer)}
                            onReveal={() => props.onRevealFile(entry().transfer)}
                            onDelete={() => props.onDeleteFile(entry().transfer)}
                            onDetails={() => props.onDetailsFile(entry().transfer)}
                            onForward={() => props.onForwardFile(entry().transfer)}
                            onLongPress={(event) => props.onFileLongPress(event, entry().transfer)}
                            onControl={(action) => props.onControl(entry().transfer, action)}
                          />
                        )}
                      </Show>
                    </MessageScrollerItem>
                  );
                }}
              </For>
            </Show>
          </MessageScrollerContent>
        </MessageScrollerViewport>
      </MessageScroller>
    </MessageScrollerProvider>
  );
}
