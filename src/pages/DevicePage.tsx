import { For, Show, createMemo } from "solid-js";
import { A, useNavigate, useParams } from "@solidjs/router";
import {
  Ban,
  MessageCircle,
  ShieldCheck,
  Users,
  Wifi,
  WifiOff,
} from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button } from "../ui";
import { shortId } from "../lib/format";
import type { PeerSummary } from "../bindings";

/**
 * The device section: a list on the left, and the selected device's details on
 * the right - the same shape as a contacts app. Details answer "what is this
 * machine and can I reach it"; talking to it is one button away.
 */
export function DevicePage() {
  const engine = useEngine();
  const { t, locale } = useI18n();
  const params = useParams();
  const navigate = useNavigate();

  const peers = () => engine.snapshot()?.peers ?? [];
  const trusted = createMemo(() =>
    peers()
      .filter((peer) => peer.trusted)
      .sort((a, b) => a.name.localeCompare(b.name)),
  );
  const nearby = createMemo(() =>
    peers()
      .filter((peer) => !peer.trusted)
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  const selected = createMemo(() => {
    const id = params.deviceId;
    if (!id) return undefined;
    return peers().find((peer) => peer.device_id === id);
  });

  const pairingBusy = () => engine.pairingBusy() || Boolean(engine.pairing());

  /// The second line of a device row. Falls back to the platform name when the
  /// peer is an older build that sends no hardware details.
  function deviceSubtitle(peer: PeerSummary): string {
    const parts = [peer.model, peer.os_version].filter(Boolean);
    if (parts.length === 0) return peer.platform;
    return parts.join(" · ");
  }

  async function pairWith(deviceId: string) {
    if (pairingBusy()) return;
    await engine.pairStart(deviceId);
  }

  function formatPairedAt(ms: number): string {
    if (!ms) return t("devices.neverPaired");
    return new Date(ms).toLocaleDateString(locale(), {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  }

  return (
    <section class="fx-devices">
      <aside class="fx-device-list" aria-label={t("devices.listAria")}>
        <header class="fx-device-list-head">
          <h2>{t("devices.title")}</h2>
          <span class="fx-count">{trusted().length}</span>
        </header>

        <div class="fx-device-list-body">
          <Show
            when={trusted().length > 0}
            fallback={
              <div class="fx-device-list-empty">
                <ShieldCheck size={18} aria-hidden="true" />
                <p>{t("devices.noTrusted")}</p>
                <span>{t("devices.noTrustedBody")}</span>
              </div>
            }
          >
            <ul class="fx-device-rows">
              <For each={trusted()}>
                {(peer) => (
                  <li>
                    <A
                      href={`/devices/${peer.device_id}`}
                      class="fx-device-row"
                      classList={{
                        "is-active": selected()?.device_id === peer.device_id,
                      }}
                    >
                      <span
                        class="fx-chat-presence"
                        classList={{ "is-online": peer.online }}
                        aria-hidden="true"
                      />
                      <span class="fx-device-row-main">
                        <strong>{peer.name}</strong>
                        <span>{deviceSubtitle(peer)}</span>
                      </span>
                      <Show when={peer.online}>
                        <ShieldCheck size={12} aria-label={t("devices.online")} />
                      </Show>
                    </A>
                  </li>
                )}
              </For>
            </ul>
          </Show>

          <Show when={nearby().length > 0}>
            <div class="fx-device-group">
              <div class="fx-device-group-head">
                <span>{t("devices.nearbyTitle")}</span>
                <span class="fx-count">{nearby().length}</span>
              </div>
              <ul class="fx-device-rows">
                <For each={nearby()}>
                  {(peer) => (
                    <li>
                      <A
                        href={`/devices/${peer.device_id}`}
                        class="fx-device-row is-unpaired"
                        classList={{
                          "is-active": selected()?.device_id === peer.device_id,
                        }}
                      >
                        <span
                          class="fx-chat-presence is-online"
                          aria-hidden="true"
                        />
                        <span class="fx-device-row-main">
                          <strong>{peer.name}</strong>
                          <span>{t("devices.unpaired")}</span>
                        </span>
                      </A>
                    </li>
                  )}
                </For>
              </ul>
            </div>
          </Show>
        </div>
      </aside>

      <Show
        when={selected()}
        fallback={
          <div class="fx-device-detail fx-device-placeholder">
            <Users size={26} aria-hidden="true" />
            <h2>{t("devices.selectTitle")}</h2>
            <p>{t("devices.selectBody")}</p>
          </div>
        }
      >
        {(peer) => (
          <article class="fx-device-detail">
            <header class="fx-device-detail-head">
              <div class="fx-device-avatar" aria-hidden="true">
                {peer().name.slice(0, 1).toUpperCase()}
              </div>
              <div>
                <h2>{peer().name}</h2>
                <p>
                  {peer().online ? (
                    <>
                      <Wifi size={13} aria-hidden="true" />
                      {t("devices.online")}
                    </>
                  ) : (
                    <>
                      <WifiOff size={13} aria-hidden="true" />
                      {t("devices.offline")}
                    </>
                  )}
                </p>
              </div>
            </header>

            <dl class="fx-device-facts">
              {/* Brand and model only when the peer reports them: an older
                  build sends nothing, and an empty row reads as a bug. */}
              <Show when={peer().manufacturer}>
                <div>
                  <dt>{t("devices.brandLabel")}</dt>
                  <dd>{peer().manufacturer}</dd>
                </div>
              </Show>
              <Show when={peer().model}>
                <div>
                  <dt>{t("devices.modelLabel")}</dt>
                  <dd>{peer().model}</dd>
                </div>
              </Show>
              <div>
                <dt>{t("devices.platformLabel")}</dt>
                <dd>{peer().os_version || peer().platform}</dd>
              </div>
              <div>
                <dt>{t("devices.idLabel")}</dt>
                <dd class="fx-numeric" title={peer().device_id}>
                  {shortId(peer().device_id, 24)}
                </dd>
              </div>
              <div>
                <dt>{t("devices.addressLabel")}</dt>
                <dd class="fx-numeric">
                  {peer().addresses.length > 0
                    ? peer().addresses.join(", ")
                    : t("devices.noAddress")}
                </dd>
              </div>
              <div>
                <dt>{t("devices.trustLabel")}</dt>
                <dd>
                  {peer().trusted
                    ? t("devices.trustedSince", {
                        date: formatPairedAt(peer().paired_at_ms),
                      })
                    : t("devices.notTrusted")}
                </dd>
              </div>
            </dl>

            <footer class="fx-device-detail-foot">
              <Show
                when={peer().trusted}
                fallback={
                  <Button
                    variant="primary"
                    aria-busy={pairingBusy()}
                    onClick={() => void pairWith(peer().device_id)}
                  >
                    {t("devices.pairAction")}
                  </Button>
                }
              >
                <Button
                  variant="primary"
                  icon={<MessageCircle size={15} />}
                  onClick={() => navigate(`/chat/${peer().device_id}`)}
                >
                  {t("devices.messageAction")}
                </Button>
                <A href="/settings/advanced" class="fx-device-trust-link">
                  <Ban size={13} aria-hidden="true" />
                  {t("devices.manageTrust")}
                </A>
              </Show>
            </footer>
          </article>
        )}
      </Show>
    </section>
  );
}
