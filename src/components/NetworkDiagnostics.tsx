import { For, Show } from "solid-js";
import { Copy, Network, ShieldAlert } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, toast } from "../ui";
import { copyText, errorDetails } from "../lib/format";

export function NetworkDiagnostics() {
  const engine = useEngine();
  const { t } = useI18n();
  const network = () => engine.snapshot()!.network;

  const stateKey = () => {
    switch (network().state) {
      case "discovering":
        return "network.stateDiscovering";
      case "listening":
        return "network.stateListening";
      case "listener_only":
        return "network.stateListenerOnly";
      case "unavailable":
        return "network.stateUnavailable";
      case "stopped":
        return "network.stateStopped";
      default:
        return "network.stateStarting";
    }
  };

  const tone = () =>
    network().state === "discovering"
      ? "is-ready"
      : network().state === "unavailable" || network().state === "stopped"
        ? "is-danger"
        : "is-warn";

  async function copyDiagnostics() {
    const status = engine.snapshot()!.status;
    const details = errorDetails({
      device_id: status.device_id,
      version: status.version,
      protocol_version: status.protocol_version,
      data_dir: status.data_dir,
      network: network(),
      snapshot_version: engine.snapshot()!.snapshot_version,
      seq: engine.snapshot()!.seq,
    });
    if (await copyText(details)) {
      toast.success(t("common.copied"));
    } else {
      toast.error(t("common.copyFailed"));
    }
  }

  // Tailwind cannot see a class built from a signal, so the tones are spelled
  // out here instead of appended to a base string.
  const statusPillClass = (value: string) => {
    const base = "rounded-full border px-2 py-0.5 text-label";
    if (value === "is-ready") return `${base} border-success/40 bg-success/10 text-success`;
    if (value === "is-warn") return `${base} border-warning/40 bg-warning/10 text-warning`;
    if (value === "is-danger")
      return `${base} border-destructive/40 bg-destructive/10 text-destructive`;
    return `${base} border-border text-muted-foreground`;
  };

  return (
    <section
      class="overflow-hidden rounded-lg border border-border bg-card"
      aria-labelledby="network-heading"
    >
      <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div>
          <h2 id="network-heading" class="text-title font-[620] tracking-[-0.012em]">
            {t("network.title")}
          </h2>
          <p class="text-body-sm text-muted-foreground">{t("network.body")}</p>
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

      <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-4 py-2 text-xs">
        <span class={statusPillClass(tone())}>
          <Network size={13} aria-hidden="true" />
          {t(stateKey())}
        </span>
        <span class="px-4 py-3 text-xs text-muted-foreground">{network().detail}</span>
      </div>

      <Show when={network().state === "unavailable" || network().state === "listener_only"}>
        <div class="rounded-lg bg-warning/10 px-3 py-2 text-xs text-warning">
          <ShieldAlert size={16} aria-hidden="true" />
          <p class="text-body-sm text-muted-foreground">{t("network.blocked")}</p>
        </div>
      </Show>

      <dl class="grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
        <div>
          <dt>{t("network.interface")}</dt>
          <dd>{network().interface ?? t("network.none")}</dd>
        </div>
        <div>
          <dt>{t("network.localAddress")}</dt>
          <dd class="font-mono text-xs">{network().local_address ?? t("network.none")}</dd>
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
          <dt>{t("network.advertised")}</dt>
          <dd>
            {network().advertised ? t("common.yes") : t("common.no")}
            {" · "}
            {network().browsing ? t("network.browsing") : t("network.notBrowsing")}
          </dd>
        </div>
        <div>
          <dt>{t("network.nearby")}</dt>
          <dd class="font-mono text-xs">{network().nearby_count}</dd>
        </div>
        <div>
          <dt>{t("network.fallback")}</dt>
          <dd>
            {network().fallback_active
              ? t("network.fallbackActive")
              : t("network.fallbackInactive")}
            {" · "}
            {t("network.fallbackStats", {
              probes: network().fallback_probes_sent,
              replies: network().fallback_responses,
            })}
          </dd>
        </div>
        <Show when={network().fallback_gateway}>
          <div>
            <dt>{t("network.fallbackGateway")}</dt>
            <dd class="font-mono text-xs">{network().fallback_gateway}</dd>
          </div>
        </Show>
        <Show when={network().fallback_peers > 0}>
          <div>
            <dt>{t("network.fallbackPeers")}</dt>
            <dd class="font-mono text-xs">{network().fallback_peers}</dd>
          </div>
        </Show>
        <Show when={network().fallback_targets.length > 0}>
          <div class="col-span-2 flex flex-col gap-1">
            <dt>{t("network.fallbackTargets")}</dt>
            <dd class="flex flex-col gap-0.5 font-mono text-xs">
              <For each={network().fallback_targets}>
                {(target) => <span class="font-mono text-xs">{target}</span>}
              </For>
            </dd>
          </div>
        </Show>
        <div class="col-span-2 flex flex-col gap-1">
          <dt>{t("network.interfaces")}</dt>
          <dd class="flex flex-col gap-0.5 font-mono text-xs">
            <Show
              when={network().local_addresses.length > 0}
              fallback={<span class="text-xs text-muted-foreground">{t("network.none")}</span>}
            >
              <For each={network().local_addresses}>
                {(address) => <span class="font-mono text-xs">{address}</span>}
              </For>
            </Show>
          </dd>
        </div>
      </dl>
      <Show when={network().fallback_last_error}>
        <p class="px-4 py-3 text-xs text-muted-foreground">
          {t("network.fallbackError", {
            message: network().fallback_last_error ?? "",
          })}
        </p>
      </Show>
      <p class="px-4 py-3 text-xs text-muted-foreground">{t("network.note")}</p>
    </section>
  );
}
