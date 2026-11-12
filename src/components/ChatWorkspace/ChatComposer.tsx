import { Show } from "solid-js";
import { Paperclip, Send, WifiOff } from "lucide-solid";
import { Button, Textarea } from "../../ui";
import { useI18n } from "../../i18n";

/**
 * The composer, or the reason it is not there.
 *
 * An offline peer gets a note instead of a disabled box: a text field that
 * cannot be used, with no explanation, reads as a broken app.
 */
export function ChatComposer(props: {
  online: boolean;
  draft: string;
  sending: boolean;
  canSend: boolean;
  onDraft: (value: string) => void;
  onSend: () => void;
  onAttach: () => void;
}) {
  const { t } = useI18n();

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    props.onSend();
  };

  return (
    <Show
      when={props.online}
      fallback={
        <div class="mx-4 mb-2 rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">
          <WifiOff size={16} aria-hidden="true" />
          <span>{t("chat.offlineHint")}</span>
        </div>
      }
    >
      <div class="border-t border-border p-3">
        <Textarea
          value={props.draft}
          onInput={(event: InputEvent & { currentTarget: HTMLTextAreaElement }) =>
            props.onDraft(event.currentTarget.value)
          }
          onKeyDown={onKeyDown}
          placeholder={t("chat.placeholder")}
          aria-label={t("chat.placeholder")}
          rows={2}
          disabled={props.sending}
        />
        <div class="mt-2 flex items-center justify-between gap-3">
          <span class="text-label text-muted-foreground">{t("chat.enterHint")}</span>
          <div class="flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              aria-label={t("chat.attach")}
              title={t("chat.attach")}
              onClick={() => props.onAttach()}
            >
              <Paperclip size={15} aria-hidden="true" />
              {t("chat.attach")}
            </Button>
            <Button
              size="sm"
              variant="primary"
              disabled={!props.canSend}
              onClick={() => props.onSend()}
            >
              <Send size={15} aria-hidden="true" />
              {props.sending ? t("chat.sending") : t("chat.send")}
            </Button>
          </div>
        </div>
      </div>
    </Show>
  );
}
