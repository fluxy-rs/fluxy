import { createEffect, For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";

import { Button, Dialog, Input, toast } from "../ui";
import { checkPermissions, requestPermissions, scan } from "@tauri-apps/plugin-barcode-scanner";
import { PairingQrDialog } from "../components/PairingQrDialog";
import { TrustedDevices } from "../components/TrustedDevices";
import { useNavigate } from "@solidjs/router";
import { takeOpenChatDevice } from "../api/engine";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import type {
  ClipboardReceivePolicy,
  DeviceAutoAccept,
  NearbyPeerView,
  PeerSummary,
  RegisteredFileView,
} from "../bindings";
import type { PairingChallenge } from "../state/engine";
import { ChevronDown, MessageCircle, QrCode, ScanLine } from "lucide-solid";
import { shortId } from "../lib/format";
import { parsePairingPayload } from "../lib/pairing";

export function MobilePage() {
  const engine = useEngine();
  const { t } = useI18n();
  const [address, setAddress] = createSignal("");
  const [fingerprint, setFingerprint] = createSignal("");
  const [challenge, setChallenge] = createSignal<PairingChallenge | null>(null);
  const [files, setFiles] = createSignal<RegisteredFileView[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
  const [nearby, setNearby] = createSignal<NearbyPeerView[]>([]);
  const [manualOpen, setManualOpen] = createSignal(false);
  const [waiting, setWaiting] = createSignal(false);
  let waitTimer: number | undefined;

  // Discovery fills this table; polling keeps it current without wiring another
  // event stream into the page.
  // A tapped chat notification deep-links to fluxy://chat; the activity parks
  // that in PendingShare and this drains it once the page can navigate.
  const checkOpenChat = async () => {
    try {
      const deviceId = await takeOpenChatDevice();
      if (deviceId) navigate(`/chat/${deviceId}`);
    } catch {
      // A failed drain is not worth a banner; the next poll tries again.
    }
  };

  onMount(() => {
    void checkOpenChat();
    // A share from another app arrives as a content URI; register it so the
    // send flow can offer it without the user picking the file again.
    //
    // Draining once on mount is not enough: a second share lands in the running
    // activity through onNewIntent, so the page has to ask again whenever it
    // comes back to the front.
    const drainShared = async () => {
      try {
        const { uris, device } = await engine.sharedFiles();
        console.warn(`[share] drained ${uris.length} uri(s) target=${device ?? "none"}`);
        const registered: RegisteredFileView[] = [];
        for (const uri of uris) {
          const view = await engine.openSourceFile(uri);
          console.warn(`[share] registered ${uri} -> ${view ? view.name : "null"}`);
          if (view) registered.push(view);
        }
        if (registered.length === 0) return;
        setFiles((current) => [...current, ...registered]);
        // A deep link that named a device is meant to run without a tap.
        if (device) {
          console.warn(`[share] auto-sending ${registered.length} file(s) to ${device}`);
          await send(device);
        }
      } catch (error) {
        console.warn("[share] failed", error);
      }
    };
    void drainShared();
    const onVisible = () => {
      void checkOpenChat();
      if (!document.hidden) void drainShared();
    };
    document.addEventListener("visibilitychange", onVisible);
    onCleanup(() => document.removeEventListener("visibilitychange", onVisible));
    // visibilitychange only fires when the page actually leaves and returns. A
    // share into the already-foreground app arrives through onNewIntent while
    // document.hidden stays false the whole time, so the listener above never
    // runs and the file is never queued. Poll the queue alongside the peer
    // refresh: sharedFiles() drains, so a poll is a no-op when nothing is new.
    let stopped = false;
    const refresh = async () => {
      try {
        // Polled rather than event-driven: visibilitychange does not fire
        // reliably when the WebView is restored from the background, and this
        // loop already runs every two seconds.
        void checkOpenChat();
        const peers = await engine.nearbyPeers();
        if (!stopped) setNearby(peers);
      } catch {
        // The list simply stays as it was; a transient failure is not worth a
        // banner on a phone.
      }
    };
    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
      void drainShared();
    }, 2500);
    onCleanup(() => {
      stopped = true;
      window.clearInterval(timer);
      window.clearTimeout(waitTimer);
    });
  });

  const navigate = useNavigate();
  // Accordion: at most one device shows its details, which matters on a phone
  // where an expanded row pushes everything else off the screen.
  const [expanded, setExpanded] = createSignal<string | null>(null);

  // The details panel edits the trust entry, which lives in the snapshot, not
  // in the discovery list: NearbyPeerView carries no policy fields.
  const trustEntry = (deviceId: string): PeerSummary | undefined =>
    snapshot()?.peers.find((peer) => peer.device_id === deviceId);

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

  async function applyClipboardPolicy(deviceId: string, policy: ClipboardReceivePolicy) {
    try {
      await engine.setClipboardPolicy(deviceId, policy);
      toast.success(t("devices.policySaved"));
    } catch (failure) {
      toast.error(failure instanceof Error ? failure.message : String(failure));
    }
  }

  async function applyAutoAccept(deviceId: string, policy: DeviceAutoAccept) {
    try {
      await engine.setDeviceAutoAccept(deviceId, policy);
      toast.success(t("devices.policySaved"));
    } catch (failure) {
      toast.error(failure instanceof Error ? failure.message : String(failure));
    }
  }

  async function removeTrust(peer: PeerSummary) {
    try {
      await engine.removeTrust(peer.device_id);
      setConfirmRemove(null);
      setExpanded(null);
      toast.success(t("devices.trustRemoved", { name: peer.name }));
    } catch (failure) {
      toast.error(failure instanceof Error ? failure.message : String(failure));
    }
  }

  const snapshot = createMemo(() => engine.snapshot());
  const status = createMemo(() => snapshot()?.status);
  const pairing = createMemo(() => engine.pairing());

  /// "Titan 2 · Android 16" reads better than a bare platform name, and falls
  /// back to it when the peer is an older build that sends no hardware hints.
  function peerSubtitle(peer: { platform: string; model: string; os_version: string }) {
    const parts = [peer.model, peer.os_version].filter(Boolean);
    return parts.length > 0 ? parts.join(" · ") : peer.platform;
  }

  // "Waiting for the other device" means the code has not been shown yet. As
  // soon as a challenge exists the screen switches to comparing digits, and
  // leaving the waiting note up next to a working SAS panel read as if the
  // request had not landed.
  createEffect(() => {
    if (pairing()) {
      window.clearTimeout(waitTimer);
      setWaiting(false);
      setNotice(null);
    }
  });
  const pairingError = createMemo(() => engine.pairingError());

  async function pair() {
    setBusy(true);
    setError(null);
    try {
      const view = await engine.pairManual(address().trim(), fingerprint().trim());
      setChallenge(view);
      setNotice(t("mobile.pairWaiting"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  /// Pair with a device discovery found: no address or fingerprint to type, the
  /// six-digit code is the whole check.
  async function pairWith(deviceId: string) {
    setBusy(true);
    setError(null);
    setNotice(t("mobile.pairWaiting"));
    setWaiting(true);
    // A request that never gets an answer must not sit forever: the page gives
    // the user a way out instead of an endless spinner.
    window.clearTimeout(waitTimer);
    waitTimer = window.setTimeout(() => {
      if (waiting()) {
        setWaiting(false);
        setNotice(null);
        setError(t("mobile.pairNoAnswer"));
      }
    }, 30000);
    try {
      await engine.pairStart(deviceId);
    } catch (cause) {
      setWaiting(false);
      setNotice(null);
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function cancelPairing() {
    window.clearTimeout(waitTimer);
    setWaiting(false);
    setNotice(null);
    try {
      await engine.pairCancel();
    } catch {
      // Nothing to cancel is not an error worth showing.
    }
  }

  async function confirm() {
    if (!challenge()) return;
    setBusy(true);
    try {
      await engine.pairConfirm();
      setChallenge(null);
      setNotice(t("mobile.pairConfirmed"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function send(deviceId: string) {
    const ids = files().map((file) => file.file_id);
    console.warn(`[send] attempt device=${deviceId} files=${ids.length}`);
    if (ids.length === 0) {
      // Silence used to look like a dead button; say something instead.
      setError(t("mobile.noFiles"));
      return;
    }
    setBusy(true);
    setError(null);
    // Hashing a multi-gigabyte file happens before the first byte moves, and on
    // a phone that is tens of seconds. Say so, or the screen looks frozen.
    setNotice(t("mobile.preparing"));
    try {
      console.warn(`[send] invoking startTransfers ids=${ids.join(",")}`);
      const summaries = await engine.startTransfers(deviceId, ids);
      console.warn(`[send] startTransfers returned ${summaries.length}`);
      setFiles([]);
      setNotice(t("mobile.sent"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  // The payload another device needs to reach this one. The fingerprint is not
  // in it on purpose: pairing still ends in a SAS comparison on both screens,
  // so a scanned code can start a connection but never establish trust.
  const [qrOpen, setQrOpen] = createSignal(false);
  /**
   * Scan a peer's code, then start pairing with whatever device it names.
   *
   * The payload only says where a device is; trust still comes from the SAS
   * comparison both screens show afterwards, so a photographed code cannot
   * pair anything on its own.
   */
  async function scanCode() {
    if (busy()) return;
    try {
      // The plugin refuses to open the camera until the runtime permission is
      // granted, and its own error for that is easy to miss.
      if ((await checkPermissions()) !== "granted") {
        const state = await requestPermissions();
        if (state !== "granted") {
          setError(t("mobile.scanNoPermission"));
          return;
        }
      }
      // Full-screen, not windowed. The windowed mode puts the preview behind
      // the WebView and makes the WebView background transparent, expecting the
      // page to act as an overlay - but this page paints an opaque background,
      // so the camera ended up completely hidden. Full screen lets the plugin
      // own the display until it decodes something.
      const result = await scan({});
      const payload = parsePairingPayload(result.content);
      if (!payload) {
        setError(t("mobile.scanNotOurs"));
        return;
      }
      // The code carries an address and the key to pin, so pairing does not
      // wait for discovery to notice the peer - which is the point of scanning
      // on a network that filters multicast.
      setError(null);
      if (payload.addr && payload.port > 0) {
        setBusy(true);
        setNotice(t("mobile.pairWaiting"));
        setWaiting(true);
        try {
          const view = await engine.pairManual(`${payload.addr}:${payload.port}`, payload.fp);
          setChallenge(view);
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
        } finally {
          setBusy(false);
        }
        return;
      }
      const known = nearby().find((peer) => peer.device_id === payload.id);
      if (!known) {
        setError(t("mobile.scanUnknown"));
        return;
      }
      await pairWith(known.device_id);
    } catch (cause) {
      // The plugin rejects with a plain object, so String(cause) prints
      // [object Object] and hides the reason. Pull the fields that exist.
      const detail = cause as { message?: string; code?: string } | null;
      const message = detail?.message ?? detail?.code ?? JSON.stringify(cause);
      // Cancelling the camera rejects too; that is not worth a banner.
      if (!message.toLowerCase().includes("cancel")) {
        setError(t("mobile.scanFailed", { message }));
      }
    }
  }

  return (
    <main class="flex flex-col gap-3">
      <header class="flex items-center gap-3 border-b border-border px-4 py-3">
        <div class="flex min-w-0 flex-1 flex-col gap-0.5">
          <h1 class="text-display font-[620] leading-[1.04] tracking-[-0.035em]">
            {t("nav.devices")}
          </h1>
          <span class="font-mono text-xs text-muted-foreground">
            {status()?.device_name} · {status()?.listen_port}
          </span>
        </div>
        {/* The two pairing actions are the only ones this page performs, and
            both are occasional. As icons in the header they stop costing a card
            of vertical space on a 1:1 screen. */}
        <div class="flex shrink-0 items-center gap-1">
          <button
            class="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            type="button"
            aria-label={t("mobile.scanAction")}
            title={t("mobile.scanAction")}
            disabled={busy()}
            onClick={() => void scanCode()}
          >
            <ScanLine size={20} aria-hidden="true" />
          </button>
          <button
            class="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            type="button"
            aria-label={t("pairing.myQrTitle")}
            title={t("pairing.myQrTitle")}
            onClick={() => setQrOpen(true)}
          >
            <QrCode size={20} aria-hidden="true" />
          </button>
        </div>
      </header>

      <Show when={error()}>
        <p class="text-xs text-destructive" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p class="text-xs text-muted-foreground">{notice()}</p>
      </Show>

      <section class="overflow-hidden rounded-lg border border-border bg-card">
        <h2 class="text-title font-[620] tracking-[-0.012em]">{t("mobile.nearbyTitle")}</h2>
        <Show
          when={nearby().length > 0}
          fallback={<p class="text-xs text-muted-foreground">{t("mobile.nearbyEmpty")}</p>}
        >
          <ul class="flex flex-col gap-2 px-4 py-3">
            <For each={nearby()}>
              {(peer) => {
                const open = () => expanded() === peer.device_id;
                return (
                  <li classList={{ "is-open": open() }}>
                    <div class="flex flex-col gap-2 rounded-lg border border-border px-3 py-2.5">
                      {/* The row body is the disclosure: tapping it reveals
                          what this machine is. Opening a conversation is a
                          separate, explicit action, so a glance at the
                          details never navigates away by accident. */}
                      <button
                        type="button"
                        class="flex items-center gap-2.5"
                        aria-expanded={open()}
                        onClick={() => setExpanded(open() ? null : peer.device_id)}
                      >
                        <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                          <strong>{peer.name}</strong>
                          <span class="text-xs text-muted-foreground">{peerSubtitle(peer)}</span>
                        </span>
                        <ChevronDown
                          size={16}
                          class="size-4 shrink-0 text-muted-foreground transition-transform"
                          aria-hidden="true"
                        />
                      </button>
                      <Show
                        when={peer.trusted}
                        fallback={
                          <Button
                            variant="primary"
                            disabled={busy()}
                            onClick={() => void pairWith(peer.device_id)}
                          >
                            {t("mobile.nearbyPair")}
                          </Button>
                        }
                      >
                        <button
                          type="button"
                          class="w-full justify-center"
                          onClick={() => navigate(`/chat/${peer.device_id}`)}
                        >
                          <MessageCircle size={15} aria-hidden="true" />
                          {t("mobile.openChat")}
                        </button>
                      </Show>
                    </div>
                    <Show when={open()}>
                      <dl class="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
                        <Show when={peer.manufacturer}>
                          <div>
                            <dt>{t("devices.brandLabel")}</dt>
                            <dd>{peer.manufacturer}</dd>
                          </div>
                        </Show>
                        <Show when={peer.model}>
                          <div>
                            <dt>{t("devices.modelLabel")}</dt>
                            <dd>{peer.model}</dd>
                          </div>
                        </Show>
                        <div>
                          <dt>{t("devices.platformLabel")}</dt>
                          <dd>{peer.os_version || peer.platform}</dd>
                        </div>
                        <div>
                          <dt>{t("devices.idLabel")}</dt>
                          <dd class="font-mono text-xs">{shortId(peer.device_id, 24)}</dd>
                        </div>
                        <Show when={peer.address}>
                          <div>
                            <dt>{t("devices.addressLabel")}</dt>
                            <dd class="font-mono text-xs">
                              {peer.address}:{peer.port}
                            </dd>
                          </div>
                        </Show>
                        <div>
                          <dt>{t("devices.trustLabel")}</dt>
                          <dd>{peer.trusted ? t("mobile.nearbyPaired") : t("devices.unpaired")}</dd>
                        </div>
                      </dl>

                      {/* Same three controls as the desktop device page,
                          stacked instead of side by side: on a phone the
                          segmented control needs the full width. */}
                      <Show when={peer.trusted && trustEntry(peer.device_id)}>
                        {(entry) => (
                          <div class="flex flex-col gap-1 px-4 py-3">
                            <div class="flex flex-col gap-1 border-b border-border py-2 last:border-0">
                              <p class="text-body font-medium">{t("devices.clipboardHeading")}</p>
                              <span class="text-xs text-muted-foreground">
                                {t("devices.clipboardBody")}
                              </span>
                              <div
                                class="flex gap-1"
                                role="group"
                                aria-label={t("devices.clipboardHeading")}
                              >
                                <For each={clipboardChoices}>
                                  {([value, label]) => (
                                    <button
                                      type="button"
                                      class="rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
                                      aria-pressed={entry().clipboard_policy === value}
                                      onClick={() =>
                                        void applyClipboardPolicy(peer.device_id, value)
                                      }
                                    >
                                      {t(label)}
                                    </button>
                                  )}
                                </For>
                              </div>
                            </div>

                            <div class="flex flex-col gap-1 border-b border-border py-2 last:border-0">
                              <p class="text-body font-medium">{t("devices.autoAcceptHeading")}</p>
                              <span class="text-xs text-muted-foreground">
                                {entry().auto_accept === "inherit"
                                  ? t("devices.autoAcceptInherit", {
                                      value: snapshot()?.settings.auto_accept_transfers
                                        ? t("devices.policyAuto")
                                        : t("devices.policyAsk"),
                                    })
                                  : t("devices.autoAcceptBody")}
                              </span>
                              <div
                                class="flex gap-1"
                                role="group"
                                aria-label={t("devices.autoAcceptHeading")}
                              >
                                <For each={autoAcceptChoices}>
                                  {([value, label]) => (
                                    <button
                                      type="button"
                                      class="rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
                                      aria-pressed={entry().auto_accept === value}
                                      onClick={() => void applyAutoAccept(peer.device_id, value)}
                                    >
                                      {t(label)}
                                    </button>
                                  )}
                                </For>
                              </div>
                            </div>

                            <div class="flex flex-col gap-1 border-t border-border pt-2">
                              <p class="text-body font-medium">{t("devices.removeTrustHeading")}</p>
                              <span class="text-xs text-muted-foreground">
                                {t("devices.removeTrustBody")}
                              </span>
                              <Button variant="outline" onClick={() => setConfirmRemove(entry())}>
                                {t("devices.removeTrustAction")}
                              </Button>
                            </div>
                          </div>
                        )}
                      </Show>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ul>
        </Show>
        <Show when={waiting()}>
          <div class="flex items-center justify-between gap-3 border-t border-border px-4 py-3">
            <p class="text-body-sm">{t("mobile.pairWaiting")}</p>
            <Button variant="outline" onClick={() => void cancelPairing()}>
              {t("mobile.pairCancel")}
            </Button>
          </div>
        </Show>
        <Show when={pairingError()}>
          {(failure) => (
            <p class="text-xs text-destructive" role="alert">
              {failure().message}
            </p>
          )}
        </Show>
        <Show when={pairing()}>
          <div class="flex items-center justify-between gap-3 border-t border-border px-4 py-3">
            <p class="text-body-sm">{t("mobile.pairCompare")}</p>
            <p class="flex items-center justify-center rounded-lg bg-muted px-3 py-2 font-mono text-sm tracking-[0.3em]">
              {pairing()?.sas}
            </p>
            <Button variant="primary" onClick={() => void confirm()}>
              {t("mobile.pairConfirm")}
            </Button>
          </div>
        </Show>
        <div class="flex flex-col gap-2 px-4 py-3">
          <Show
            when={manualOpen()}
            fallback={
              <Button variant="ghost" onClick={() => setManualOpen(true)}>
                {t("mobile.manualOpen")}
              </Button>
            }
          >
            <p class="text-body-sm text-muted-foreground">{t("mobile.manualBody")}</p>
            <label class="flex flex-col gap-1.5 px-4 py-3">
              <span>{t("mobile.address")}</span>
              <Input
                value={address()}
                placeholder="192.168.1.8:49784"
                onInput={(value) => setAddress(String(value))}
              />
            </label>
            <label class="flex flex-col gap-1.5 px-4 py-3">
              <span>{t("mobile.fingerprint")}</span>
              <Input
                value={fingerprint()}
                placeholder="64 hex characters"
                onInput={(value) => setFingerprint(String(value))}
              />
            </label>
            <Button variant="primary" disabled={busy()} onClick={() => void pair()}>
              {t("mobile.pairAction")}
            </Button>
            <Show when={challenge()}>
              <div class="flex items-center justify-between gap-3 border-t border-border px-4 py-3">
                <p class="text-body-sm">{t("mobile.pairCompare")}</p>
                <p class="flex items-center justify-center rounded-lg bg-muted px-3 py-2 font-mono text-sm tracking-[0.3em]">
                  {challenge()?.sas}
                </p>
                <Button variant="primary" onClick={() => void confirm()}>
                  {t("mobile.pairConfirm")}
                </Button>
              </div>
            </Show>
          </Show>
        </div>
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

      <PairingQrDialog open={qrOpen()} onOpenChange={setQrOpen} />
      <details class="overflow-hidden rounded-lg border border-border bg-card">
        <summary>{t("devices.trustedTitle")}</summary>
        <TrustedDevices />
      </details>
    </main>
  );
}
