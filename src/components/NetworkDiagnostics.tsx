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

  return (
    <section class="fx-panel fx-network-panel" aria-labelledby="network-heading">
      <header class="fx-panel-head">
        <div>
          <h2 id="network-heading">{t("network.title")}</h2>
          <p>{t("network.body")}</p>
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

      <div class="fx-network-banner">
        <span class={`fx-status-pill ${tone()}`}>
          <Network size={13} aria-hidden="true" />
          {t(stateKey())}
        </span>
        <span class="fx-network-detail">{network().detail}</span>
      </div>

      <Show when={network().state === "unavailable" || network().state === "listener_only"}>
        <div class="fx-network-warning">
          <ShieldAlert size={16} aria-hidden="true" />
          <p>{t("network.blocked")}</p>
        </div>
      </Show>

      <dl class="fx-definition-list fx-definition-2col">
        <div>
          <dt>{t("network.interface")}</dt>
          <dd>{network().interface ?? t("network.none")}</dd>
        </div>
        <div>
          <dt>{t("network.localAddress")}</dt>
          <dd class="fx-numeric">{network().local_address ?? t("network.none")}</dd>
        </div>
        <div>
          <dt>{t("network.listenAddr")}</dt>
          <dd class="fx-numeric">{network().listen_addr ?? t("network.none")}</dd>
        </div>
        <div>
          <dt>{t("network.service")}</dt>
          <dd class="fx-numeric">{network().mdns_service}</dd>
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
          <dd class="fx-numeric">{network().nearby_count}</dd>
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
            <dd class="fx-numeric">{network().fallback_gateway}</dd>
          </div>
        </Show>
        <Show when={network().fallback_peers > 0}>
          <div>
            <dt>{t("network.fallbackPeers")}</dt>
            <dd class="fx-numeric">{network().fallback_peers}</dd>
          </div>
        </Show>
        <Show when={network().fallback_targets.length > 0}>
          <div class="fx-definition-wide">
            <dt>{t("network.fallbackTargets")}</dt>
            <dd class="fx-network-addresses">
              <For each={network().fallback_targets}>
                {(target) => <span class="fx-numeric">{target}</span>}
              </For>
            </dd>
          </div>
        </Show>
        <div class="fx-definition-wide">
          <dt>{t("network.interfaces")}</dt>
          <dd class="fx-network-addresses">
            <Show
              when={network().local_addresses.length > 0}
              fallback={<span class="fx-muted">{t("network.none")}</span>}
            >
              <For each={network().local_addresses}>
                {(address) => <span class="fx-numeric">{address}</span>}
              </For>
            </Show>
          </dd>
        </div>
      </dl>
      <Show when={network().fallback_last_error}>
        <p class="fx-note">
          {t("network.fallbackError", {
            message: network().fallback_last_error ?? "",
          })}
        </p>
      </Show>
      <p class="fx-note">{t("network.note")}</p>
    </section>
  );
}
