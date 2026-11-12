import { Show } from "solid-js";
import type { TransferAction, TransferSummary } from "../../bindings";
import { useI18n } from "../../i18n";
import { formatBytes, formatClock, formatSpeed } from "../../lib/format";
import { fileKind } from "../../lib/file-kind";
import { failureReason } from "../../lib/transfer-errors";
import { createLongPress } from "../../lib/long-press";
import {
  Attachment,
  AttachmentContent,
  AttachmentDescription,
  AttachmentMedia,
  AttachmentTitle,
} from "../../ui/attachment";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuPortal,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "../../ui/context-menu";
import { Message, MessageContent } from "../../ui/message";
import { TransferActions } from "../TransferActions";
import { attachmentState, transferLabel } from "./ChatWorkspace.utils";

/**
 * A file in the conversation.
 *
 * It carries two menus for the same four choices: a right click on a machine
 * with a mouse, and a long press on a phone. The phone one is owned by the
 * workspace, because the panel it opens hangs over the whole conversation.
 */
export function FileCard(props: {
  transfer: TransferSummary;
  busy: boolean;
  canForward: boolean;
  desktopMenuDisabled: boolean;
  onOpen: () => void;
  onReveal: () => void;
  onDelete: () => void;
  onDetails: () => void;
  onForward: () => void;
  onLongPress: (event: PointerEvent) => void;
  onControl: (action: TransferAction) => void;
}) {
  const { t, locale } = useI18n();
  const press = createLongPress((event) => props.onLongPress(event));
  const sent = () => props.transfer.direction === "send";

  return (
    <Message align={sent() ? "end" : "start"}>
      <MessageContent>
        {/* Only a finished transfer has all four choices: there is nothing to
            reveal or forward while a file is still moving. */}
        <ContextMenu disabled={props.desktopMenuDisabled || props.transfer.state !== "completed"}>
          <ContextMenuTrigger class="contents">
            <Attachment
              state={attachmentState(props.transfer.state)}
              onContextMenu={(event) => event.preventDefault()}
              onPointerDown={(event) => press.start(event)}
              onPointerMove={(event) => press.move(event)}
              onPointerUp={press.cancel}
              onPointerCancel={press.cancel}
              class="max-w-[min(22rem,80%)] cursor-pointer"
              role={openable(props.transfer) ? "button" : undefined}
              tabindex={openable(props.transfer) ? 0 : undefined}
              onClick={() => {
                if (openable(props.transfer)) props.onOpen();
              }}
              onKeyDown={(event) => {
                if (!openable(props.transfer)) return;
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                props.onOpen();
              }}
            >
              <AttachmentSummary transfer={props.transfer} />
            </Attachment>
          </ContextMenuTrigger>
          <ContextMenuPortal>
            <ContextMenuContent>
              <ContextMenuItem onClick={() => props.onReveal()}>
                {t("transfers.menuReveal")}
              </ContextMenuItem>
              <ContextMenuItem variant="destructive" onClick={() => props.onDelete()}>
                {t("transfers.menuDelete")}
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem onClick={() => props.onDetails()}>
                {t("transfers.menuDetails")}
              </ContextMenuItem>
              <ContextMenuItem disabled={!props.canForward} onClick={() => props.onForward()}>
                {t("transfers.menuForward")}
              </ContextMenuItem>
            </ContextMenuContent>
          </ContextMenuPortal>
        </ContextMenu>
        <div
          class="flex flex-wrap items-center gap-2 text-label text-muted-foreground"
          classList={{ "self-end": sent(), "flex-row-reverse": sent() }}
        >
          <time class="font-mono text-label">
            {formatClock(props.transfer.created_at_ms, locale())}
          </time>
          <TransferActions
            transfer={props.transfer}
            busy={props.busy}
            onAction={(action) => props.onControl(action)}
          />
        </div>
      </MessageContent>
    </Message>
  );
}

// Only a received, finished file is worth opening: the sender's copy is
// already theirs, and a file still moving cannot be handed to another app.
function openable(transfer: TransferSummary) {
  return transfer.direction === "receive" && transfer.state === "completed";
}

/** The card's ink: name, size, state, and the progress a transfer shows. */
function AttachmentSummary(props: { transfer: TransferSummary }) {
  const { t, locale } = useI18n();
  // A batch walks through its files one at a time, so each new file re-enters
  // `preparing` while the cumulative byte counters keep climbing. Both states
  // are "moving".
  const moving = () =>
    props.transfer.state === "transferring" || props.transfer.state === "preparing";
  const percent = () =>
    props.transfer.bytes_total > 0
      ? Math.round((props.transfer.bytes_done / props.transfer.bytes_total) * 100)
      : 0;

  return (
    <>
      <AttachmentMedia>
        {(() => {
          const Kind = fileKind(props.transfer.file_name).icon;
          return <Kind size={18} aria-hidden="true" />;
        })()}
      </AttachmentMedia>
      <AttachmentContent>
        <AttachmentTitle>{props.transfer.file_name}</AttachmentTitle>
        <AttachmentDescription>
          {formatBytes(props.transfer.bytes_total)} · {transferLabel(props.transfer, t)}
          <span classList={{ invisible: !moving() }}>
            {" · "}
            {percent()}%
            <Show when={props.transfer.bytes_per_second > 0}>
              {" · "}
              {formatSpeed(props.transfer.bytes_per_second)}
            </Show>
          </span>
        </AttachmentDescription>
        {/* The track stays mounted and only its ink hides: unmounting it
            mid-batch collapses the card and makes the scrollbar jump. */}
        <span
          class="mt-1 block h-1 w-full overflow-hidden rounded-full bg-muted"
          classList={{ invisible: !moving() }}
          aria-hidden={!moving()}
        >
          <span class="block h-full rounded-full bg-primary" style={{ width: `${percent()}%` }} />
        </span>
        <Show when={props.transfer.state === "failed"}>
          <span class="text-label text-destructive">
            {failureReason(
              props.transfer.error_code,
              locale(),
              props.transfer.error_message ?? t("chat.errorGeneric"),
            )}
          </span>
        </Show>
      </AttachmentContent>
    </>
  );
}
