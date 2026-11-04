import { For, Show, createSignal } from "solid-js";
import { FileText, RefreshCw } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button } from "../ui";

/// Shows the tail of the session log.
///
/// A release build has nothing attached to its stderr, so without this the only
/// way to learn why something failed on a phone is to rebuild it as a debug
/// build - which is exactly the situation this panel exists to end.
export function LogPanel() {
  const engine = useEngine();
  const { t } = useI18n();
  const [lines, setLines] = createSignal<string[] | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);

  async function load() {
    setBusy(true);
    setFailure(null);
    try {
      setLines(await engine.appLogTail(400));
    } catch (cause) {
      setFailure(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section class="fx-panel fx-panel-wide" aria-labelledby="log-heading">
      <h2 id="log-heading">{t("settings.logHeading")}</h2>
      <p class="fx-muted">{t("settings.logHint")}</p>
      <div class="fx-log-actions">
        <Button
          variant="outline"
          onClick={() => void load()}
          disabled={busy()}
          icon={busy() ? <RefreshCw size={14} class="fx-spin" /> : <FileText size={14} />}
        >
          {lines() === null ? t("settings.logLoad") : t("settings.logRefresh")}
        </Button>
      </div>
      <Show when={failure()}>
        <p class="fx-form-error" role="alert">
          {failure()}
        </p>
      </Show>
      <Show when={lines()}>
        {(entries) => (
          <Show
            when={entries().length > 0}
            fallback={<p class="fx-muted">{t("settings.logEmpty")}</p>}
          >
            <pre class="fx-log" aria-label={t("settings.logHeading")}>
              <For each={entries()}>{(line) => <div>{line}</div>}</For>
            </pre>
          </Show>
        )}
      </Show>
    </section>
  );
}
