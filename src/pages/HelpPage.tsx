import { For, Show } from "solid-js";
import { Copy, LifeBuoy, MonitorSmartphone, ShieldAlert, Waypoints } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { RoutedSheet } from "../components/RoutedSheet";
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
    <RoutedSheet title={t("help.title")}>
      <div class="flex flex-col gap-4">
        <header class="flex flex-col gap-1">
          <div>
            <h1 class="text-heading font-[620] tracking-[-0.015em]">{t("help.title")}</h1>
            <p class="text-body-sm text-muted-foreground">{t("help.body")}</p>
          </div>
        </header>

        <section
          class="overflow-hidden rounded-lg border border-border bg-card"
          aria-labelledby="help-background-heading"
        >
          <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h2 id="help-background-heading">{t("help.backgroundTitle")}</h2>
              <p class="text-body-sm text-muted-foreground">{t("help.backgroundBody")}</p>
            </div>
            <MonitorSmartphone size={16} aria-hidden="true" />
          </header>
          <ul class="flex flex-col gap-2 px-4 py-3">
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

        <section
          class="overflow-hidden rounded-lg border border-border bg-card"
          aria-labelledby="help-firewall-heading"
        >
          <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h2 id="help-firewall-heading">{t("help.portsTitle")}</h2>
              <p class="text-body-sm text-muted-foreground">{t("help.portsBody")}</p>
            </div>
            <ShieldAlert size={16} aria-hidden="true" />
          </header>

          <dl class="grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
            <div>
              <dt>{t("help.portDiscovery")}</dt>
              <dd class="font-mono text-xs">UDP {settings().discovery_port}</dd>
            </div>
            <div>
              <dt>{t("help.portTransfer")}</dt>
              <dd class="font-mono text-xs">UDP {transferPort()}</dd>
            </div>
          </dl>
          <p class="text-xs text-muted-foreground">{t("help.portNote")}</p>

          <div class="rounded-lg border border-border px-4 py-3">
            <div class="flex items-center gap-2 text-sm font-medium">
              <h3 class="text-body font-semibold">{t("help.windowsTitle")}</h3>
              <Button
                variant="outline"
                size="xs"
                icon={<Copy size={13} />}
                onClick={() => void copy(windowsCommand())}
              >
                {t("help.copyRule")}
              </Button>
            </div>
            <p class="text-body-sm text-muted-foreground">{t("help.windowsBody")}</p>
            <pre class="overflow-x-auto rounded bg-muted px-2 py-1 font-mono text-xs whitespace-pre-wrap break-all">
              {windowsCommand()}
            </pre>
          </div>

          <div class="rounded-lg border border-border px-4 py-3">
            <div class="flex items-center gap-2 text-sm font-medium">
              <h3 class="text-body font-semibold">{t("help.macosTitle")}</h3>
              <Button
                variant="outline"
                size="xs"
                icon={<Copy size={13} />}
                onClick={() => void copy(t("help.macosCommand"))}
              >
                {t("help.copyRule")}
              </Button>
            </div>
            <p class="text-body-sm text-muted-foreground">{t("help.macosBody")}</p>
            <pre class="overflow-x-auto rounded bg-muted px-2 py-1 font-mono text-xs whitespace-pre-wrap break-all">
              {t("help.macosCommand")}
            </pre>
          </div>

          <div class="rounded-lg border border-border px-4 py-3">
            <div class="flex items-center gap-2 text-sm font-medium">
              <h3 class="text-body font-semibold">{t("help.linuxTitle")}</h3>
              <Button
                variant="outline"
                size="xs"
                icon={<Copy size={13} />}
                onClick={() => void copy(linuxCommand())}
              >
                {t("help.copyRule")}
              </Button>
            </div>
            <p class="text-body-sm text-muted-foreground">{t("help.linuxBody")}</p>
            <pre class="overflow-x-auto rounded bg-muted px-2 py-1 font-mono text-xs whitespace-pre-wrap break-all">
              {linuxCommand()}
            </pre>
          </div>
        </section>

        <section
          class="overflow-hidden rounded-lg border border-border bg-card"
          aria-labelledby="help-trouble-heading"
        >
          <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h2 id="help-trouble-heading">{t("help.troubleTitle")}</h2>
              <p class="text-body-sm text-muted-foreground">{t("help.troubleBody")}</p>
            </div>
            <Waypoints size={16} aria-hidden="true" />
          </header>
          <dl class="flex flex-col gap-1 border-b border-border px-4 py-3 last:border-0">
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

        <section
          class="overflow-hidden rounded-lg border border-border bg-card"
          aria-labelledby="help-diagnostics-heading"
        >
          <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h2 id="help-diagnostics-heading">{t("help.diagnosticsTitle")}</h2>
              <p class="text-body-sm text-muted-foreground">{t("help.diagnosticsBody")}</p>
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
          <dl class="grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
            <div>
              <dt>{t("help.dataDir")}</dt>
              <dd class="font-mono text-xs">{engine.snapshot()!.status.data_dir}</dd>
            </div>
            <div>
              <dt>{t("network.listenAddr")}</dt>
              <dd class="font-mono text-xs">{network().listen_addr ?? t("network.none")}</dd>
            </div>
            <div>
              <dt>{t("network.service")}</dt>
              <dd class="font-mono text-xs">{network().mdns_service}</dd>
            </div>
            <div>
              <dt>{t("help.docsNote")}</dt>
              <dd class="font-mono text-xs">docs/FIREWALL.md</dd>
            </div>
          </dl>
          <Show when={!network().advertised || !network().browsing}>
            <p class="rounded-lg border border-[var(--warning)]/30 bg-[var(--warning)]/5 px-4 py-3 text-xs text-[var(--warning)]">
              <LifeBuoy size={14} aria-hidden="true" /> {t("help.notAdvertised")}
            </p>
          </Show>
        </section>
      </div>
    </RoutedSheet>
  );
}
