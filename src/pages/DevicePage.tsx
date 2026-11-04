import { For, Show, createMemo, createSignal } from "solid-js";
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
import { Button, Dialog, toast } from "../ui";
import { orderDevices } from "../lib/device-order";
import { shortId } from "../lib/format";
import type { ClipboardReceivePolicy, DeviceAutoAccept, PeerSummary } from "../bindings";

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

  /// When this device last sent or received a message. Zero for a pairing that
  /// has never been used, which the ordering below replaces with paired_at.
  function lastMessageAt(deviceId: string): number {
    return (
      engine
        .chatConversations()
        .find((conversation) => conversation.device_id === deviceId)
        ?.last_at_ms ?? 0
    );
  }

  const ordered = createMemo(() =>
    orderDevices(peers(), lastMessageAt, locale()),
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

  const [confirmRemove, setConfirmRemove] = createSignal<PeerSummary | null>(null);

  const clipboardChoices: Array<[ClipboardReceivePolicy, string]> = [
    ["ask", "devices.policyAsk"],
    ["auto", "devices.policyAuto"],
    ["reject", "devices.policyReject"],
  ];

  const autoAcceptChoices: Array<[DeviceAutoAccept, string]> = [
    ["inherit", "devices.policyInherit"],
    ["auto", "devices.policyAuto"],
    ["ask", "devices.policyAsk"],
  ];

  async function applyClipboardPolicy(peer: PeerSummary, policy: ClipboardReceivePolicy) {
    if (peer.clipboard_policy === policy) return;
    try {
      await engine.setClipboardPolicy(peer.device_id, policy);
      toast.success(t("devices.policySaved"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }

  async function applyAutoAccept(peer: PeerSummary, policy: DeviceAutoAccept) {
    if (peer.auto_accept === policy) return;
    try {
      await engine.setDeviceAutoAccept(peer.device_id, policy);
      toast.success(t("devices.policySaved"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }

  async function removeTrust(peer: PeerSummary) {
    try {
      await engine.removeTrust(peer.device_id);
      setConfirmRemove(null);
      navigate("/devices");
      toast.success(t("devices.trustRemoved", { name: peer.name }));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <>
      <section class="fx-devices">
      <aside class="fx-device-list" aria-label={t("devices.listAria")}>
        <header class="fx-device-list-head">
          <h2>{t("devices.title")}</h2>
          <span class="fx-count">{ordered().length}</span>
        </header>

        <div class="fx-device-list-body">
          <Show
            when={ordered().length > 0}
            fallback={
              <div class="fx-device-list-empty">
                <ShieldCheck size={18} aria-hidden="true" />
                <p>{t("devices.noTrusted")}</p>
                <span>{t("devices.noTrustedBody")}</span>
              </div>
            }
          >
            <ul class="fx-device-rows">
              <For each={ordered()}>
                {(peer) => (
                  <li>
                    <A
                      href={`/devices/${peer.device_id}`}
                      class="fx-device-row"
                      classList={{
                        "is-active": selected()?.device_id === peer.device_id,
                        "is-unpaired": !peer.trusted,
                      }}
                    >
                      <span
                        class="fx-chat-presence"
                        classList={{ "is-online": peer.online }}
                        aria-hidden="true"
                      />
                      <span class="fx-device-row-main">
                        <strong>{peer.name}</strong>
                        <span>
                          {peer.trusted
                            ? deviceSubtitle(peer)
                            : t("devices.unpaired")}
                        </span>
                      </span>
                      <Show when={peer.trusted && peer.online}>
                        <ShieldCheck size={12} aria-label={t("devices.online")} />
                      </Show>
                    </A>
                  </li>
                )}
              </For>
            </ul>
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

            <Show when={peer().trusted}>
              <section class="fx-device-controls">
                <h3>{t("devices.receiveHeading")}</h3>
                <div class="fx-device-control">
                  <div class="fx-device-control-text">
                    <p>{t("devices.clipboardHeading")}</p>
                    <span>{t("devices.clipboardBody")}</span>
                  </div>
                  <div class="fx-choice" role="group" aria-label={t("devices.clipboardHeading")}>
                    <For each={clipboardChoices}>
                      {([value, label]) => (
                        <button
                          type="button"
                          class="fx-choice-item"
                          aria-pressed={peer().clipboard_policy === value}
                          onClick={() => void applyClipboardPolicy(peer(), value)}
                        >
                          {t(label)}
                        </button>
                      )}
                    </For>
                  </div>
                </div>

                <div class="fx-device-control">
                  <div class="fx-device-control-text">
                    <p>{t("devices.autoAcceptHeading")}</p>
                    <span>
                      {peer().auto_accept === "inherit"
                        ? t("devices.autoAcceptInherit", {
                            value: engine.snapshot()?.settings.auto_accept_transfers
                              ? t("devices.policyAuto")
                              : t("devices.policyAsk"),
                          })
                        : t("devices.autoAcceptBody")}
                    </span>
                  </div>
                  <div class="fx-choice" role="group" aria-label={t("devices.autoAcceptHeading")}>
                    <For each={autoAcceptChoices}>
                      {([value, label]) => (
                        <button
                          type="button"
                          class="fx-choice-item"
                          aria-pressed={peer().auto_accept === value}
                          onClick={() => void applyAutoAccept(peer(), value)}
                        >
                          {t(label)}
                        </button>
                      )}
                    </For>
                  </div>
                </div>

                <div class="fx-device-control is-danger">
                  <div class="fx-device-control-text">
                    <p>{t("devices.removeTrustHeading")}</p>
                    <span>{t("devices.removeTrustBody")}</span>
                  </div>
                  <Button
                    variant="ghost"
                    icon={<Ban size={14} aria-hidden="true" />}
                    onClick={() => setConfirmRemove(peer())}
                  >
                    {t("devices.removeTrustAction")}
                  </Button>
                </div>
              </section>
            </Show>

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
              </Show>
            </footer>
          </article>
        )}
      </Show>

      </section>

      <Dialog
        open={confirmRemove() !== null}
        onOpenChange={(open) => {
          if (!open) setConfirmRemove(null);
        }}
        title={t("devices.removeTrustTitle")}
        description={t("devices.removeTrustConfirm", {
          name: confirmRemove()?.name ?? "",
        })}
        width="sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmRemove(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                const target = confirmRemove();
                if (target) void removeTrust(target);
              }}
            >
              {t("devices.removeTrustAction")}
            </Button>
          </>
        }
      />
    </>
  );
}
