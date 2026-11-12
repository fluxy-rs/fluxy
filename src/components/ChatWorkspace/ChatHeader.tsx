import { Show } from "solid-js";
import { ChevronLeft, Link2, Wifi, WifiOff } from "lucide-solid";
import { useI18n } from "../../i18n";

/** Who is on the other end, and whether they are reachable right now. */
export function ChatHeader(props: {
  name: string;
  online: boolean;
  connected: boolean;
  compact: boolean;
  onBack: () => void;
}) {
  const { t } = useI18n();
  const state = () =>
    props.online ? (props.connected ? t("chat.connected") : t("chat.online")) : t("chat.offline");

  return (
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <Show when={props.compact}>
        <button
          type="button"
          class="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          aria-label={t("chat.back")}
          onClick={() => props.onBack()}
        >
          <ChevronLeft size={18} aria-hidden="true" />
        </button>
      </Show>
      <div>
        <h2 class="text-heading font-[620] tracking-[-0.015em]">{props.name}</h2>
        <p class="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Show when={props.online} fallback={<WifiOff size={12} aria-hidden="true" />}>
            <Show when={props.connected} fallback={<Wifi size={12} aria-hidden="true" />}>
              <Link2 size={12} aria-hidden="true" />
            </Show>
          </Show>
          <span>{state()}</span>
        </p>
      </div>
      <span class="rounded-full border border-border px-2 py-0.5 text-label text-muted-foreground">
        {t("chat.memoryPill")}
      </span>
    </header>
  );
}
