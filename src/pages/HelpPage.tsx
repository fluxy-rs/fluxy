import { For, Show } from "solid-js";
import {
  Copy,
  LifeBuoy,
  MonitorSmartphone,
  ShieldAlert,
  Waypoints,
} from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, toast } from "../ui";
import { copyText, errorDetails } from "../lib/format";

/** Windows and Linux rules are literal shell syntax, so they live in the
 * catalog with the ports substituted at render time. */
function fill(template: string, values: Record<string, string>): string {
  return Object.entries(values).reduce(
    (text, [key, value]) => text.split(`{${key}}`).join(value),
    template,
  );
}

export function HelpPage() {
  const engine = useEngine();
  const { t } = useI18n();
  const settings = () => engine.snapshot()!.settings;
  const network = () => engine.snapshot()!.network;

  /** The bound transfer port, when the listener already published one. */
  const transferPort = () => {
    const bound = network().listen_addr ?? "";
    const port = bound.slice(bound.lastIndexOf(":") + 1);
    return port || String(settings().listen_port || 0);
  };

  const rules = () => ({
    discovery: String(settings().discovery_port),
    port: transferPort(),
    exe: t("help.executablePlaceholder"),
  });

  async function copy(value: string) {
    if (await copyText(value)) {
      toast.success(t("common.copied"));
    } else {
      toast.error(t("common.copyFailed"));
    }
  }

  const windowsCommand = () =>
    fill(t("help.windowsCommand"), {
      discovery: rules().discovery,
      port: rules().port,
    });
  const linuxCommand = () =>
    fill(t("help.linuxCommand"), {
      discovery: rules().discovery,
      port: rules().port,
    });

  async function copyDiagnostics() {
    const status = engine.snapshot()!.status;
    const details = errorDetails({
      device_id: status.device_id,
      version: status.version,
      protocol_version: status.protocol_version,
      data_dir: status.data_dir,
      network: network(),
      autostart: settings().autostart,
      autostart_location: settings().autostart_location,
      snapshot_version: engine.snapshot()!.snapshot_version,
      seq: engine.snapshot()!.seq,
    });
    await copy(details);
  }

  return (
    <div class="fx-page">
      <header class="fx-page-head">
        <div>
          <h1>{t("help.title")}</h1>
          <p>{t("help.body")}</p>
        </div>
      </header>

      <section class="fx-panel" aria-labelledby="help-background-heading">
        <header class="fx-panel-head">
          <div>
            <h2 id="help-background-heading">{t("help.backgroundTitle")}</h2>
            <p>{t("help.backgroundBody")}</p>
          </div>
          <MonitorSmartphone size={16} aria-hidden="true" />
        </header>
        <ul class="fx-help-list">
          <li>{t("help.backgroundTray")}</li>
          <li>{t("help.backgroundQuit")}</li>
          <li>
            {settings().autostart
              ? t("help.autostartOn", {
                  location: settings().autostart_location,
                })
              : t("help.autostartOff")}
          </li>
        </ul>
      </section>

      <section class="fx-panel" aria-labelledby="help-firewall-heading">
        <header class="fx-panel-head">
          <div>
            <h2 id="help-firewall-heading">{t("help.portsTitle")}</h2>
            <p>{t("help.portsBody")}</p>
          </div>
          <ShieldAlert size={16} aria-hidden="true" />
        </header>

        <dl class="fx-definition-list fx-definition-2col">
          <div>
            <dt>{t("help.portDiscovery")}</dt>
            <dd class="fx-numeric">UDP {settings().discovery_port}</dd>
          </div>
          <div>
            <dt>{t("help.portTransfer")}</dt>
            <dd class="fx-numeric">UDP {transferPort()}</dd>
          </div>
        </dl>
        <p class="fx-note">{t("help.portNote")}</p>

        <div class="fx-help-rule">
          <div class="fx-help-rule-head">
            <h3>{t("help.windowsTitle")}</h3>
            <Button
              variant="outline"
              size="xs"
              icon={<Copy size={13} />}
              onClick={() => void copy(windowsCommand())}
            >
              {t("help.copyRule")}
            </Button>
          </div>
          <p>{t("help.windowsBody")}</p>
          <pre class="fx-help-command">{windowsCommand()}</pre>
        </div>

        <div class="fx-help-rule">
          <div class="fx-help-rule-head">
            <h3>{t("help.macosTitle")}</h3>
            <Button
              variant="outline"
              size="xs"
              icon={<Copy size={13} />}
              onClick={() => void copy(t("help.macosCommand"))}
            >
              {t("help.copyRule")}
            </Button>
          </div>
          <p>{t("help.macosBody")}</p>
          <pre class="fx-help-command">{t("help.macosCommand")}</pre>
        </div>

        <div class="fx-help-rule">
          <div class="fx-help-rule-head">
            <h3>{t("help.linuxTitle")}</h3>
            <Button
              variant="outline"
              size="xs"
              icon={<Copy size={13} />}
              onClick={() => void copy(linuxCommand())}
            >
              {t("help.copyRule")}
            </Button>
          </div>
          <p>{t("help.linuxBody")}</p>
          <pre class="fx-help-command">{linuxCommand()}</pre>
        </div>
      </section>

      <section class="fx-panel" aria-labelledby="help-trouble-heading">
        <header class="fx-panel-head">
          <div>
            <h2 id="help-trouble-heading">{t("help.troubleTitle")}</h2>
            <p>{t("help.troubleBody")}</p>
          </div>
          <Waypoints size={16} aria-hidden="true" />
        </header>
        <dl class="fx-help-faq">
          <For
            each={[
              ["help.troubleMdns", "help.troubleMdnsBody"],
              ["help.troubleFirewall", "help.troubleFirewallBody"],
              ["help.troublePublic", "help.troublePublicBody"],
              ["help.troubleVpn", "help.troubleVpnBody"],
              ["help.troubleHotspot", "help.troubleHotspotBody"],
            ]}
          >
            {([question, answer]) => (
              <div>
                <dt>{t(question)}</dt>
                <dd>{t(answer, { discovery: rules().discovery })}</dd>
              </div>
            )}
          </For>
        </dl>
      </section>

      <section class="fx-panel" aria-labelledby="help-diagnostics-heading">
        <header class="fx-panel-head">
          <div>
            <h2 id="help-diagnostics-heading">{t("help.diagnosticsTitle")}</h2>
            <p>{t("help.diagnosticsBody")}</p>
          </div>
          <Button
            variant="outline"
            size="xs"
            icon={<Copy size={13} />}
            onClick={() => void copyDiagnostics()}
          >
            {t("settings.copyDiagnostics")}
          </Button>
        </header>
        <dl class="fx-definition-list fx-definition-2col">
          <div>
            <dt>{t("help.dataDir")}</dt>
            <dd class="fx-numeric">{engine.snapshot()!.status.data_dir}</dd>
          </div>
          <div>
            <dt>{t("network.listenAddr")}</dt>
            <dd class="fx-numeric">
              {network().listen_addr ?? t("network.none")}
            </dd>
          </div>
          <div>
            <dt>{t("network.service")}</dt>
            <dd class="fx-numeric">{network().mdns_service}</dd>
          </div>
          <div>
            <dt>{t("help.docsNote")}</dt>
            <dd class="fx-numeric">docs/FIREWALL.md</dd>
          </div>
        </dl>
        <Show when={!network().advertised || !network().browsing}>
          <p class="fx-policy-warning">
            <LifeBuoy size={14} aria-hidden="true" /> {t("help.notAdvertised")}
          </p>
        </Show>
      </section>
    </div>
  );
}
