import { platformLabel } from "../lib/platform";
import { For, Show, createMemo, createSignal } from "solid-js";
import { A, useNavigate, useParams } from "@solidjs/router";
import { Ban, MessageCircle, ShieldCheck, Users, Wifi, WifiOff } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, toast } from "../ui";
import { Empty, EmptyDescription, EmptyMedia, EmptyTitle } from "../ui/empty";
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
      engine.chatConversations().find((conversation) => conversation.device_id === deviceId)
        ?.last_at_ms ?? 0
    );
  }

  const ordered = createMemo(() => orderDevices(peers(), lastMessageAt, locale()));

  const selected = createMemo(() => {
    const id = params.deviceId;
    if (!id) return undefined;
    return peers().find((peer) => peer.device_id === id);
  });

  const pairingBusy = () => engine.pairingBusy() || Boolean(engine.pairing());

  /// The second line of a device row. Falls back to the platform name when the
  /// peer is an older build that sends no hardware details.
  function deviceSubtitle(peer: PeerSummary): string {
    const hardware = [peer.manufacturer, peer.model].filter(Boolean).join(" ");
    const address = peer.addresses[0] ?? "";
    return [hardware, address].filter(Boolean).join(" · ");
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
      <section class="grid min-h-0 flex-1 grid-cols-[minmax(15rem,18.75rem)_minmax(0,1fr)] overflow-hidden rounded-lg border border-border bg-card">
        <aside
          class="flex min-h-0 flex-col border-r border-border bg-muted/30"
          aria-label={t("devices.listAria")}
        >
          <header class="flex items-center justify-between gap-2.5 px-4 pt-3.5 pb-2.5">
            <h2 class="text-title font-[620] tracking-[-0.012em]">{t("devices.title")}</h2>
            <span class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
              {ordered().length}
            </span>
          </header>

          <div class="min-h-0 flex-1 overflow-y-auto pb-2.5">
            <Show
              when={ordered().length > 0}
              fallback={
                <Empty class="p-4">
                  <EmptyMedia variant="icon">
                    <ShieldCheck size={18} aria-hidden="true" />
                  </EmptyMedia>
                  <EmptyTitle>{t("devices.noTrusted")}</EmptyTitle>
                  <EmptyDescription>{t("devices.noTrustedBody")}</EmptyDescription>
                </Empty>
              }
            >
              <ul class="flex flex-col">
                <For each={ordered()}>
                  {(peer) => (
                    <li>
                      <A
                        href={`/devices/${peer.device_id}`}
                        class="flex w-full items-center gap-2.5 px-4 py-2.5 text-left transition-colors hover:bg-muted aria-[current=page]:bg-accent"
                      >
                        <span
                          class="size-2 shrink-0 rounded-full"
                          classList={{
                            "bg-success": peer.online,
                            "bg-muted-foreground/40": !peer.online,
                          }}
                          aria-hidden="true"
                        />
                        <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                          <span class="flex min-w-0 items-center gap-1.5">
                            <strong class="truncate text-body font-semibold">{peer.name}</strong>
                            <span class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-micro text-muted-foreground">
                              {platformLabel(peer.platform)}
                            </span>
                          </span>
                          <span class="truncate text-label text-muted-foreground">
                            {peer.trusted ? deviceSubtitle(peer) : t("devices.unpaired")}
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
            <div class="flex flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
              <Users size={26} aria-hidden="true" />
              <h2 class="text-heading font-[620] tracking-[-0.015em]">
                {t("devices.selectTitle")}
              </h2>
              <p class="text-body-sm text-muted-foreground">{t("devices.selectBody")}</p>
            </div>
          }
        >
          {(peer) => (
            <article class="flex min-h-0 flex-col gap-4 overflow-y-auto p-6">
              <header class="flex items-center gap-3 border-b border-border pb-4">
                <div
                  class="grid size-12 shrink-0 place-items-center rounded-full bg-accent text-lg font-semibold text-accent-foreground"
                  aria-hidden="true"
                >
                  {peer().name.slice(0, 1).toUpperCase()}
                </div>
                <div>
                  <h2 class="text-heading font-[620] tracking-[-0.015em]">{peer().name}</h2>
                  <p class="flex items-center gap-1.5 text-body-sm text-muted-foreground">
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

              <dl class="grid grid-cols-[9rem_minmax(0,1fr)] gap-x-4 gap-y-3">
                {/* Brand and model only when the peer reports them: an older
                  build sends nothing, and an empty row reads as a bug. */}
                <Show when={peer().manufacturer}>
                  <div>
                    <dt class="text-label text-muted-foreground">{t("devices.brandLabel")}</dt>
                    <dd class="text-body">{peer().manufacturer}</dd>
                  </div>
                </Show>
                <Show when={peer().model}>
                  <div>
                    <dt class="text-label text-muted-foreground">{t("devices.modelLabel")}</dt>
                    <dd class="text-body">{peer().model}</dd>
                  </div>
                </Show>
                <div>
                  <dt class="text-label text-muted-foreground">{t("devices.platformLabel")}</dt>
                  <dd class="text-body">{peer().os_version || peer().platform}</dd>
                </div>
                <div>
                  <dt class="text-label text-muted-foreground">{t("devices.idLabel")}</dt>
                  <dd class="font-mono text-xs" title={peer().device_id}>
                    {shortId(peer().device_id, 24)}
                  </dd>
                </div>
                <div>
                  <dt class="text-label text-muted-foreground">{t("devices.addressLabel")}</dt>
                  <dd class="font-mono text-xs">
                    {peer().addresses.length > 0
                      ? peer().addresses.join(", ")
                      : t("devices.noAddress")}
                  </dd>
                </div>
                <div>
                  <dt class="text-label text-muted-foreground">{t("devices.trustLabel")}</dt>
                  <dd class="text-body">
                    {peer().trusted
                      ? t("devices.trustedSince", {
                          date: formatPairedAt(peer().paired_at_ms),
                        })
                      : t("devices.notTrusted")}
                  </dd>
                </div>
              </dl>

              <Show when={peer().trusted}>
                <section class="mt-5 border-t border-border pt-4">
                  <h3 class="text-body font-semibold">{t("devices.receiveHeading")}</h3>
                  <div class="flex items-center justify-between gap-4 py-2.5">
                    <div class="min-w-0">
                      <p class="text-body font-medium">{t("devices.clipboardHeading")}</p>
                      <span class="text-label text-muted-foreground">
                        {t("devices.clipboardBody")}
                      </span>
                    </div>
                    <div
                      class="flex overflow-hidden rounded-lg border border-border bg-background"
                      role="group"
                      aria-label={t("devices.clipboardHeading")}
                    >
                      <For each={clipboardChoices}>
                        {([value, label]) => (
                          <button
                            type="button"
                            class="px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground aria-pressed:bg-primary aria-pressed:text-primary-foreground"
                            aria-pressed={peer().clipboard_policy === value}
                            onClick={() => void applyClipboardPolicy(peer(), value)}
                          >
                            {t(label)}
                          </button>
                        )}
                      </For>
                    </div>
                  </div>

                  <div class="flex items-center justify-between gap-4 py-2.5">
                    <div class="min-w-0">
                      <p class="text-body font-medium">{t("devices.autoAcceptHeading")}</p>
                      <span class="text-label text-muted-foreground">
                        {peer().auto_accept === "inherit"
                          ? t("devices.autoAcceptInherit", {
                              value: engine.snapshot()?.settings.auto_accept_transfers
                                ? t("devices.policyAuto")
                                : t("devices.policyAsk"),
                            })
                          : t("devices.autoAcceptBody")}
                      </span>
                    </div>
                    <div
                      class="flex overflow-hidden rounded-lg border border-border bg-background"
                      role="group"
                      aria-label={t("devices.autoAcceptHeading")}
                    >
                      <For each={autoAcceptChoices}>
                        {([value, label]) => (
                          <button
                            type="button"
                            class="px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground aria-pressed:bg-primary aria-pressed:text-primary-foreground"
                            aria-pressed={peer().auto_accept === value}
                            onClick={() => void applyAutoAccept(peer(), value)}
                          >
                            {t(label)}
                          </button>
                        )}
                      </For>
                    </div>
                  </div>

                  <div class="flex items-center justify-between gap-4 border-t border-border py-2.5">
                    <div class="min-w-0">
                      <p class="text-body font-medium text-destructive">
                        {t("devices.removeTrustHeading")}
                      </p>
                      <span class="text-label text-muted-foreground">
                        {t("devices.removeTrustBody")}
                      </span>
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

              <footer class="mt-auto flex items-center gap-3 border-t border-border pt-4">
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
