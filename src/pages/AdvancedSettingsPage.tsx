import { Show, createSignal, onMount } from "solid-js";
import { ArrowLeft, Copy } from "lucide-solid";
import { useEngine } from "../state/engine";
import { LogPanel } from "../components/LogPanel";
import { NetworkDiagnostics } from "../components/NetworkDiagnostics";
import { NetworkSettings } from "../components/NetworkSettings";
import { useI18n } from "../i18n";
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
    <div class="fx-page fx-page-narrow">
      <header class="fx-page-head">
        <div>
          <a class="fx-back-link" href="/settings">
            <ArrowLeft size={15} aria-hidden="true" />
            {t("settings.backToSettings")}
          </a>
          <h1>{t("settings.advancedTitle")}</h1>
          <p>{t("settings.advancedBody")}</p>
        </div>
      </header>

      <div class="fx-settings-grid">
        <NetworkSettings />

        <section class="fx-panel fx-panel-wide" aria-labelledby="identity-heading">
          <header class="fx-panel-head">
            <div>
              <h2 id="identity-heading">{t("settings.identityTitle")}</h2>
              <p>{t("settings.identityBody")}</p>
            </div>
          </header>
          <dl class="fx-definition-list fx-definition-2col">
            <div>
              <dt>{t("settings.nameLabel")}</dt>
              <dd>{status().device_name}</dd>
            </div>
            <div>
              <dt>{t("settings.idLabel")}</dt>
              <dd class="fx-numeric" title={status().device_id}>
                {shortId(status().device_id, 24)}
              </dd>
            </div>
            <div>
              <dt>{t("settings.protocolLabel")}</dt>
              <dd class="fx-numeric">v{status().protocol_version}</dd>
            </div>
            <div>
              <dt>{t("settings.portLabel")}</dt>
              <dd class="fx-numeric">
                {status().listen_port === 0
                  ? t("home.systemAssigned")
                  : status().listen_port}
              </dd>
            </div>
            <div class="fx-definition-wide">
              <dt>{t("settings.dataDirLabel")}</dt>
              <dd class="fx-path">{status().data_dir}</dd>
            </div>
          </dl>
        </section>

        <NetworkDiagnostics />

        <section class="fx-panel" aria-labelledby="diagnostics-heading">
          <header class="fx-panel-head">
            <div>
              <h2 id="diagnostics-heading">{t("settings.diagnosticsTitle")}</h2>
              <p>{t("settings.diagnosticsBody")}</p>
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
          <p class="fx-label">{t("home.thisDevice")}</p>
          <dl class="fx-definition-list">
            <div>
              <dt>{t("home.deviceId")}</dt>
              <dd class="fx-id-line">
                <span class="fx-numeric">{shortId(snapshot().status.device_id, 22)}</span>
                <button
                  class="fx-inline-copy"
                  type="button"
                  aria-label={t("home.copyId")}
                  onClick={() => void copyDeviceId()}
                >
                  <Show
                    when={copiedId()}
                    fallback={<Copy size={13} aria-hidden="true" />}
                  >
                    <span class="fx-copied-mark">{t("common.copied")}</span>
                  </Show>
                </button>
              </dd>
            </div>
            <div>
              <dt>{t("home.protocol")}</dt>
              <dd class="fx-numeric">v{snapshot().status.protocol_version}</dd>
            </div>
            <div>
              <dt>{t("home.listenPort")}</dt>
              <dd class="fx-numeric">
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
              <dd class="fx-numeric">{engine.eventCount()}</dd>
            </div>
            <div class="fx-definition-wide">
              <dt>{t("home.dataDir")}</dt>
              <dd class="fx-path">{snapshot().status.data_dir}</dd>
            </div>
            <div class="fx-definition-wide">
              <dt>{t("home.protocol")} · loopback</dt>
              <dd>
                {snapshot().status.dev_loopback
                  ? t("home.loopbackOn")
                  : t("home.loopbackOff")}
              </dd>
            </div>
            <div>
              <dt>{t("settings.eventSeq")}</dt>
              <dd class="fx-numeric">
                {engine.lastEvent()?.seq ?? snapshot().seq}
              </dd>
            </div>
            <div>
              <dt>{t("settings.snapshotVersion")}</dt>
              <dd class="fx-numeric">v{snapshot().snapshot_version}</dd>
            </div>
            <div>
              <dt>{t("settings.lastEvent")}</dt>
              <dd class="fx-numeric">
                {engine.lastEvent()
                  ? eventName(engine.lastEvent()!)
                  : t("home.waitingEvent")}
              </dd>
            </div>
          </dl>
        </section>

        <Show when={networkDenied()}>
          <section class="fx-panel fx-panel-wide fx-banner-warning" aria-labelledby="lan-heading">
            <h2 id="lan-heading">{t("settings.lanTitle")}</h2>
            <p>{t("settings.lanBody")}</p>
            <p class="fx-muted">{t("settings.lanFix")}</p>
          </section>
        </Show>

        <LogPanel />
      </div>
    </div>
  );
}
