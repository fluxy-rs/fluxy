import { For, Show } from "solid-js";
import { MessageCircle, ShieldCheck } from "lucide-solid";
import type { ChatConversationView } from "../../bindings";
import { useI18n } from "../../i18n";
import { formatClock } from "../../lib/format";

/** The left column: who the user can talk to, and the last thing that happened. */
export function ConversationList(props: {
  conversations: ChatConversationView[];
  selectedId: string | null;
  onSelect: (deviceId: string) => void;
  displayName: (deviceId: string, fallback: string) => string;
  lastPreview: (deviceId: string) => { label: string | null; at: number; isFile: boolean };
}) {
  const { t, locale } = useI18n();

  return (
    <aside
      class="flex min-h-0 flex-col border-border bg-muted/30"
      classList={{ "border-r": true }}
      aria-label={t("chat.listAria")}
    >
      <header class="flex items-center justify-between gap-2.5 px-4 pt-3.5 pb-2.5">
        <div class="flex min-w-0 flex-col gap-0.5">
          <h2 class="text-title font-[620] tracking-[-0.012em]">{t("chat.title")}</h2>
          <p class="text-body-sm text-muted-foreground">{t("chat.memoryNote")}</p>
        </div>
        <span class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
          {props.conversations.length}
        </span>
      </header>
      <Show
        when={props.conversations.length > 0}
        fallback={
          <div class="flex flex-col items-center gap-1 p-4 text-center text-muted-foreground">
            <MessageCircle size={18} aria-hidden="true" />
            <p class="text-body font-medium">{t("chat.noConversations")}</p>
            <span class="text-label">{t("chat.noConversationsBody")}</span>
          </div>
        }
      >
        <ul class="flex min-h-0 flex-1 flex-col overflow-y-auto pb-2">
          <For each={props.conversations}>
            {(peer) => {
              const preview = () => props.lastPreview(peer.device_id);
              return (
                <li>
                  <button
                    type="button"
                    class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left transition-colors hover:bg-muted aria-[current=page]:bg-accent"
                    aria-current={props.selectedId === peer.device_id ? "true" : undefined}
                    onClick={() => props.onSelect(peer.device_id)}
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
                        {props.displayName(peer.device_id, peer.name)}
                      </strong>
                      <span
                        class="text-label text-muted-foreground"
                        classList={{ "font-mono": preview().isFile }}
                      >
                        {preview().label ?? t("chat.noMessages")}
                      </span>
                    </span>
                    <span class="flex shrink-0 flex-col items-end gap-0.5 text-label text-muted-foreground">
                      <Show when={preview().at > 0}>
                        <time class="font-mono text-label">
                          {formatClock(preview().at, locale())}
                        </time>
                      </Show>
                      <Show when={peer.online}>
                        <ShieldCheck size={12} aria-label={t("chat.online")} />
                      </Show>
                    </span>
                  </button>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </aside>
  );
}
