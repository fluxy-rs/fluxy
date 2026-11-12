import { Show } from "solid-js";
import type { ChatMessageView } from "../../bindings";
import { useI18n } from "../../i18n";
import { formatClock } from "../../lib/format";
import { createLongPress } from "../../lib/long-press";
import { Bubble, BubbleContent } from "../../ui/bubble";
import { Message, MessageContent } from "../../ui/message";
import { errorLabel, statusLabel } from "./ChatWorkspace.utils";

/** A text message, with its delivery state and a retry when it failed. */
export function TextBubble(props: {
  message: ChatMessageView;
  peerName: string;
  peerOnline: boolean;
  retrying: boolean;
  selectable: boolean;
  onLongPress: (event: PointerEvent) => void;
  onRetry: () => void;
}) {
  const { t, locale } = useI18n();
  const press = createLongPress((event) => props.onLongPress(event));
  const sent = () => props.message.direction === "sent";

  return (
    <Message align={sent() ? "end" : "start"}>
      <MessageContent>
        <Bubble
          variant={sent() ? "default" : "muted"}
          data-message-id={props.message.message_id}
          classList={{ "select-text": props.selectable }}
          // Without these the WebView starts its own text selection on a long
          // press and shows a system toolbar instead of this menu.
          class="select-none [-webkit-touch-callout:none]"
          onContextMenu={(event) => event.preventDefault()}
          onPointerDown={(event) => press.start(event)}
          onPointerMove={(event) => press.move(event)}
          onPointerUp={press.cancel}
          onPointerCancel={press.cancel}
        >
          <BubbleContent class="px-3 py-2 text-sm break-words">{props.message.text}</BubbleContent>
        </Bubble>
        <div
          class="flex flex-wrap items-center gap-2 text-label text-muted-foreground"
          classList={{ "self-end": sent(), "flex-row-reverse": sent() }}
        >
          <time class="font-mono text-label">
            {formatClock(props.message.created_at_ms, locale())}
          </time>
          <span classList={{ "font-semibold": props.message.state === "failed" }}>
            {statusLabel(props.message, t)}
          </span>
          <Show when={props.message.state === "failed" && props.message.retryable}>
            <button
              type="button"
              class="text-xs text-primary underline-offset-2 hover:underline"
              disabled={props.retrying || !props.peerOnline}
              title={!props.peerOnline ? t("chat.offlineHint") : undefined}
              aria-label={t("chat.retryAria", { name: props.peerName })}
              onClick={() => props.onRetry()}
            >
              {props.retrying ? t("chat.retrying") : t("chat.retry")}
            </button>
          </Show>
          <Show when={props.message.state === "failed" && props.message.error_message}>
            <p class="text-label text-destructive">{errorLabel(props.message, t)}</p>
          </Show>
        </div>
      </MessageContent>
    </Message>
  );
}
