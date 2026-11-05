import { Show, createSignal, onMount } from "solid-js";
import { ArrowLeft, Copy } from "lucide-solid";
import { useEngine } from "../state/engine";
import { LogPanel } from "../components/LogPanel";
import { NetworkDiagnostics } from "../components/NetworkDiagnostics";
import { NetworkSettings } from "../components/NetworkSettings";
import { useI18n } from "../i18n";
import { RoutedSheet } from "../components/RoutedSheet";
import { Button, toast } from "../ui";
import { copyText, errorDetails, eventName, shortId } from "../lib/format";

/**
 * The settings that are looked at, not adjusted: the interface in use, this
 * device identity, discovery state, the diagnostic counters and the log tail.
 *
 * All of it used to sit on the main settings page, where five panels of
 * read-mostly detail pushed the switches a user actually changes off the first
 * screen - worse on a phone, where the same content is four scrolls deep.
 */
export function AdvancedSettingsPage() {
  const engine = useEngine();
  const { t } = useI18n();
  const [copiedId, setCopiedId] = createSignal(false);
  const snapshot = () => engine.snapshot()!;
  const status = () => snapshot().status;
  // Android 16 gates LAN access behind a runtime permission; without it
  // discovery finds nothing and looks like an empty network.
  const [networkDenied, setNetworkDenied] = createSignal(false);
  onMount(() => {
    void (async () => {
      try {
        setNetworkDenied(!(await engine.localNetworkGranted()));
      } catch {
        // Older platforms have no such permission; nothing to report.
      }
    })();
  });
  const diagnostics = () =>
    errorDetails({
      version: status().version,
      device_id: status().device_id,
      protocol_version: status().protocol_version,
      listen_port: status().listen_port,
      data_dir: status().data_dir,
      snapshot_version: snapshot().snapshot_version,
      seq: snapshot().seq,
      events_received: engine.eventCount(),
      latest_event: engine.lastEvent()
        ? {
            name: eventName(engine.lastEvent()!),
            seq: engine.lastEvent()!.seq,
            emitted_at_ms: engine.lastEvent()!.emitted_at_ms,
          }
        : null,
    });

  /// Everything a bug report needs, as one block of text.
  async function copyDiagnostics() {
    if (await copyText(diagnostics())) {
      toast.success(t("common.copied"));
    } else {
      toast.error(t("common.copyFailed"));
    }
  }

  async function copyDeviceId() {
    await copyText(snapshot().status.device_id);
    setCopiedId(true);
    setTimeout(() => setCopiedId(false), 1600);
  }

  return (
    <RoutedSheet title={t("settings.advancedTitle")}>
      <div class="flex flex-col gap-4">
        <header class="flex flex-col gap-1">
          <div>
            <a
              class="inline-flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
              href="/settings"
            >
              <ArrowLeft size={15} aria-hidden="true" />
              {t("settings.backToSettings")}
            </a>
            <h1 class="text-display font-[620] leading-[1.04] tracking-[-0.035em]">
              {t("settings.advancedTitle")}
            </h1>
            <p class="text-body-sm text-muted-foreground">{t("settings.advancedBody")}</p>
          </div>
        </header>

        <div class="flex flex-col gap-4">
          <NetworkSettings />

          <section
            class="overflow-hidden rounded-lg border border-border bg-card"
            aria-labelledby="identity-heading"
          >
            <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
              <div>
                <h2 id="identity-heading" class="text-title font-[620] tracking-[-0.012em]">
                  {t("settings.identityTitle")}
                </h2>
                <p class="text-body-sm text-muted-foreground">{t("settings.identityBody")}</p>
              </div>
            </header>
            <dl class="grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
              <div>
                <dt>{t("settings.nameLabel")}</dt>
                <dd>{status().device_name}</dd>
              </div>
              <div>
                <dt>{t("settings.idLabel")}</dt>
                <dd class="font-mono text-xs" title={status().device_id}>
                  {shortId(status().device_id, 24)}
                </dd>
              </div>
              <div>
                <dt>{t("settings.protocolLabel")}</dt>
                <dd class="font-mono text-xs">v{status().protocol_version}</dd>
              </div>
              <div>
                <dt>{t("settings.portLabel")}</dt>
                <dd class="font-mono text-xs">
                  {status().listen_port === 0 ? t("home.systemAssigned") : status().listen_port}
                </dd>
              </div>
              <div class="col-span-2 flex flex-col gap-1">
                <dt>{t("settings.dataDirLabel")}</dt>
                <dd class="truncate font-mono text-xs text-muted-foreground">
                  {status().data_dir}
                </dd>
              </div>
            </dl>
          </section>

          <NetworkDiagnostics />

          <section
            class="overflow-hidden rounded-lg border border-border bg-card"
            aria-labelledby="diagnostics-heading"
          >
            <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
              <div>
                <h2 id="diagnostics-heading">{t("settings.diagnosticsTitle")}</h2>
                <p class="text-body-sm text-muted-foreground">{t("settings.diagnosticsBody")}</p>
              </div>
              <Button
                variant="outline"
                size="xs"
                onClick={() => void copyDiagnostics()}
                icon={<Copy size={13} />}
              >
                {t("settings.copyDiagnostics")}
              </Button>
            </header>
            <p class="text-xs font-medium text-muted-foreground">{t("home.thisDevice")}</p>
            <dl class="grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
              <div>
                <dt>{t("home.deviceId")}</dt>
                <dd class="flex items-center gap-2">
                  <span class="font-mono text-xs">{shortId(snapshot().status.device_id, 22)}</span>
                  <button
                    class="rounded-lg px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                    type="button"
                    aria-label={t("home.copyId")}
                    onClick={() => void copyDeviceId()}
                  >
                    <Show when={copiedId()} fallback={<Copy size={13} aria-hidden="true" />}>
                      <span class="text-xs text-[var(--success)]">{t("common.copied")}</span>
                    </Show>
                  </button>
                </dd>
              </div>
              <div>
                <dt>{t("home.protocol")}</dt>
                <dd class="font-mono text-xs">v{snapshot().status.protocol_version}</dd>
              </div>
              <div>
                <dt>{t("home.listenPort")}</dt>
                <dd class="font-mono text-xs">
                  {snapshot().status.listen_port === 0
                    ? t("home.systemAssigned")
                    : snapshot().status.listen_port}
                </dd>
              </div>
              <div>
                <dt>{t("home.discovery")}</dt>
                <dd>{t("home.discoveryMdns")}</dd>
              </div>
              <div>
                <dt>{t("home.events")}</dt>
                <dd class="font-mono text-xs">{engine.eventCount()}</dd>
              </div>
              <div class="col-span-2 flex flex-col gap-1">
                <dt>{t("home.dataDir")}</dt>
                <dd class="truncate font-mono text-xs text-muted-foreground">
                  {snapshot().status.data_dir}
                </dd>
              </div>
              <div class="col-span-2 flex flex-col gap-1">
                <dt>{t("home.protocol")} · loopback</dt>
                <dd>
                  {snapshot().status.dev_loopback ? t("home.loopbackOn") : t("home.loopbackOff")}
                </dd>
              </div>
              <div>
                <dt>{t("settings.eventSeq")}</dt>
                <dd class="font-mono text-xs">{engine.lastEvent()?.seq ?? snapshot().seq}</dd>
              </div>
              <div>
                <dt>{t("settings.snapshotVersion")}</dt>
                <dd class="font-mono text-xs">v{snapshot().snapshot_version}</dd>
              </div>
              <div>
                <dt>{t("settings.lastEvent")}</dt>
                <dd class="font-mono text-xs">
                  {engine.lastEvent() ? eventName(engine.lastEvent()!) : t("home.waitingEvent")}
                </dd>
              </div>
            </dl>
          </section>

          <Show when={networkDenied()}>
            <section
              class="rounded-lg border border-[var(--warning)]/30 bg-[var(--warning)]/5 px-4 py-3"
              aria-labelledby="lan-heading"
            >
              <h2 id="lan-heading" class="text-sm font-semibold text-[var(--warning)]">
                {t("settings.lanTitle")}
              </h2>
              <p class="mt-1 text-xs text-[var(--warning)]">{t("settings.lanBody")}</p>
              <p class="text-xs text-muted-foreground">{t("settings.lanFix")}</p>
            </section>
          </Show>

          <LogPanel />
        </div>
      </div>
    </RoutedSheet>
  );
}
