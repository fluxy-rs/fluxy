import { For, Show } from "solid-js";
import { LoaderCircle, Radio, ShieldCheck, Unplug, Wifi } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { EmptyState } from "./EmptyState";
import { Button } from "../ui";
import { shortId } from "../lib/format";

export function NearbyDevices(props: { compact?: boolean }) {
  const engine = useEngine();
  const { t } = useI18n();
  const nearby = () => (engine.snapshot()?.peers ?? []).filter((peer) => !peer.trusted);
  const pairingBusy = () => engine.pairingBusy() || Boolean(engine.pairing());

  function pair(deviceId: string) {
    // The button stays focusable while a challenge is pending so that closing
    // the dialog returns focus to the control that opened it.
    if (pairingBusy()) return;
    void engine.pairStart(deviceId);
  }

  return (
    <Show
      when={!props.compact}
      fallback={
        <section class="fx-rail-section fx-rail-nearby" aria-labelledby="nearby-rail-heading">
          <div class="fx-rail-heading">
            <span id="nearby-rail-heading">{t("rail.nearby")}</span>
            <span class="fx-count">{nearby().length}</span>
          </div>
          <Show
            when={nearby().length > 0}
            fallback={
              <div class="fx-rail-empty">
                <p>{t("rail.noNearby")}</p>
                <span>{t("rail.noNearbyBody")}</span>
              </div>
            }
          >
            <ul class="fx-peer-list fx-nearby-list">
              <For each={nearby()}>
                {(peer) => (
                  <li>
                    <div class="fx-peer-main">
                      <span class="fx-peer-name" title={peer.name}>{peer.name}</span>
                      <span class="fx-peer-id fx-numeric">
                        {peer.platform} · {shortId(peer.device_id)}
                      </span>
                    </div>
                    <button
                      class="fx-peer-pair"
                      type="button"
                      aria-label={t("pairing.pairWith", { name: peer.name })}
                      title={t("pairing.pairWith", { name: peer.name })}
                      aria-disabled={pairingBusy() || undefined}
                      aria-busy={
                        engine.pairing()?.peer_device_id === peer.device_id ||
                        undefined
                      }
                      onClick={() => pair(peer.device_id)}
                    >
                      <Show
                        when={engine.pairing()?.peer_device_id === peer.device_id}
                        fallback={<Radio size={14} aria-hidden="true" />}
                      >
                        <LoaderCircle size={14} class="fx-spin" aria-hidden="true" />
                      </Show>
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </section>
      }
    >
      <section class="fx-panel" aria-labelledby="nearby-heading">
        <header class="fx-panel-head">
          <div>
            <h2 id="nearby-heading">{t("nearby.title")}</h2>
            <p>{t("nearby.body")}</p>
          </div>
          <span class="fx-count">{nearby().length}</span>
        </header>
        <Show
          when={nearby().length > 0}
          fallback={
            <EmptyState
              icon={<Wifi size={22} />}
              title={t("nearby.emptyTitle")}
              body={t("nearby.emptyBody")}
              note={t("nearby.emptyNote")}
            />
          }
        >
          <ul class="fx-nearby-grid">
            <For each={nearby()}>
              {(peer) => (
                <li class="fx-nearby-card">
                  <div class="fx-nearby-icon" aria-hidden="true">
                    <Unplug size={17} />
                  </div>
                  <div class="fx-nearby-main">
                    <div class="fx-nearby-name">{peer.name}</div>
                    <div class="fx-nearby-meta">
                      <span>{peer.platform}</span>
                      <span class="fx-numeric">{shortId(peer.device_id, 18)}</span>
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="xs"
                    aria-disabled={pairingBusy() || undefined}
                    aria-busy={
                      engine.pairing()?.peer_device_id === peer.device_id ||
                      undefined
                    }
                    onClick={() => pair(peer.device_id)}
                    icon={
                      engine.pairing()?.peer_device_id === peer.device_id ? (
                        <LoaderCircle size={13} class="fx-spin" />
                      ) : (
                        <ShieldCheck size={13} />
                      )
                    }
                  >
                    {t("pairing.pairAction")}
                  </Button>
                </li>
              )}
            </For>
          </ul>
        </Show>
        <p class="fx-note">{t("nearby.note")}</p>
      </section>
    </Show>
  );
}
