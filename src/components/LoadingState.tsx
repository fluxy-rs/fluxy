import { For } from "solid-js";
import { useI18n } from "../i18n";

/**
 * Placeholder shown while the first snapshot is on its way.
 *
 * Three bars of decreasing width read as a list that has not painted yet,
 * which is calmer than a spinner in an empty page. The text is here for screen
 * readers and for the moment the wait is long enough to notice.
 */
export function LoadingState() {
  const { t } = useI18n();
  return (
    <div class="flex flex-col gap-3 p-4" aria-busy="true" aria-live="polite">
      <p class="text-sm font-medium">{t("loading.title")}</p>
      <p class="text-xs text-muted-foreground">{t("loading.body")}</p>
      <For each={[0, 1, 2]}>
        {(index) => (
          <div
            class="h-3 animate-pulse rounded bg-muted"
            style={{ width: `${80 - index * 15}%` }}
          />
        )}
      </For>
    </div>
  );
}
