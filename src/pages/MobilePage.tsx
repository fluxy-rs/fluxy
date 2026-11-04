import { createEffect, For, Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";

import { Button } from "../ui";
import {
  checkPermissions,
  requestPermissions,
  scan,
} from "@tauri-apps/plugin-barcode-scanner";
import { PairingQrDialog } from "../components/PairingQrDialog";
import { TrustedDevices } from "../components/TrustedDevices";
import { useNavigate } from "@solidjs/router";
import { takeOpenChatDevice } from "../api/engine";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import type { NearbyPeerView, RegisteredFileView } from "../bindings";
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
          const view = await engine.pairManual(
            `${payload.addr}:${payload.port}`,
            payload.fp,
          );
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
      const message =
        detail?.message ?? detail?.code ?? JSON.stringify(cause);
      // Cancelling the camera rejects too; that is not worth a banner.
      if (!message.toLowerCase().includes("cancel")) {
        setError(t("mobile.scanFailed", { message }));
      }
    }
  }


  return (
    <main class="fx-mobile">
      <header class="fx-mobile-head fx-mobile-head-row">
        <div class="fx-mobile-head-titles">
          <h1>{t("nav.devices")}</h1>
          <span class="fx-muted fx-numeric">
            {status()?.device_name} · {status()?.listen_port}
          </span>
        </div>
        {/* The two pairing actions are the only ones this page performs, and
            both are occasional. As icons in the header they stop costing a card
            of vertical space on a 1:1 screen. */}
        <div class="fx-mobile-head-actions">
          <button
            class="fx-icon-button"
            type="button"
            aria-label={t("mobile.scanAction")}
            title={t("mobile.scanAction")}
            disabled={busy()}
            onClick={() => void scanCode()}
          >
            <ScanLine size={20} aria-hidden="true" />
          </button>
          <button
            class="fx-icon-button"
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
        <p class="fx-form-error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p class="fx-form-hint">{notice()}</p>
      </Show>

      <section class="fx-panel">
        <h2>{t("mobile.nearbyTitle")}</h2>
        <Show
          when={nearby().length > 0}
          fallback={<p class="fx-muted">{t("mobile.nearbyEmpty")}</p>}
        >
          <ul class="fx-mobile-peers">
            <For each={nearby()}>
              {(peer) => {
                const open = () => expanded() === peer.device_id;
                return (
                  <li classList={{ "is-open": open() }}>
                    <div class="fx-mobile-peer">
                      {/* The row body is the disclosure: tapping it reveals
                          what this machine is. Opening a conversation is a
                          separate, explicit action, so a glance at the
                          details never navigates away by accident. */}
                      <button
                        type="button"
                        class="fx-mobile-peer-main"
                        aria-expanded={open()}
                        onClick={() =>
                          setExpanded(open() ? null : peer.device_id)
                        }
                      >
                        <span class="fx-mobile-peer-copy">
                          <strong>{peer.name}</strong>
                          <span class="fx-muted">{peerSubtitle(peer)}</span>
                        </span>
                        <ChevronDown
                          size={16}
                          class="fx-mobile-caret"
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
                          class="fx-mobile-chat-button"
                          onClick={() => navigate(`/chat/${peer.device_id}`)}
                        >
                          <MessageCircle size={15} aria-hidden="true" />
                          {t("mobile.openChat")}
                        </button>
                      </Show>
                    </div>
                    <Show when={open()}>
                      <dl class="fx-mobile-facts">
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
                          <dd class="fx-numeric">
                            {shortId(peer.device_id, 24)}
                          </dd>
                        </div>
                        <Show when={peer.address}>
                          <div>
                            <dt>{t("devices.addressLabel")}</dt>
                            <dd class="fx-numeric">
                              {peer.address}:{peer.port}
                            </dd>
                          </div>
                        </Show>
                        <div>
                          <dt>{t("devices.trustLabel")}</dt>
                          <dd>
                            {peer.trusted
                              ? t("mobile.nearbyPaired")
                              : t("devices.unpaired")}
                          </dd>
                        </div>
                      </dl>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ul>
        </Show>
        <Show when={waiting()}>
          <div class="fx-mobile-confirm">
            <p>{t("mobile.pairWaiting")}</p>
            <Button variant="outline" onClick={() => void cancelPairing()}>
              {t("mobile.pairCancel")}
            </Button>
          </div>
        </Show>
        <Show when={pairingError()}>
          {(failure) => (
            <p class="fx-form-error" role="alert">
              {failure().message}
            </p>
          )}
        </Show>
        <Show when={pairing()}>
          <div class="fx-mobile-confirm">
            <p>{t("mobile.pairCompare")}</p>
            <p class="fx-mobile-sas fx-numeric">{pairing()?.sas}</p>
            <Button variant="primary" onClick={() => void confirm()}>
              {t("mobile.pairConfirm")}
            </Button>
          </div>
        </Show>
        <div class="fx-mobile-manual">
        <Show
          when={manualOpen()}
          fallback={
            <Button variant="ghost" onClick={() => setManualOpen(true)}>
              {t("mobile.manualOpen")}
            </Button>
          }
        >
          <p>{t("mobile.manualBody")}</p>
        <label class="fx-field">
          <span>{t("mobile.address")}</span>
          <input
            class="fx-input"
            value={address()}
            placeholder="192.168.1.8:49784"
            onInput={(event) => setAddress(event.currentTarget.value)}
          />
        </label>
        <label class="fx-field">
          <span>{t("mobile.fingerprint")}</span>
          <input
            class="fx-input"
            value={fingerprint()}
            placeholder="64 hex characters"
            onInput={(event) => setFingerprint(event.currentTarget.value)}
          />
        </label>
          <Button variant="primary" disabled={busy()} onClick={() => void pair()}>
            {t("mobile.pairAction")}
          </Button>
          <Show when={challenge()}>
            <div class="fx-mobile-confirm">
              <p>{t("mobile.pairCompare")}</p>
              <p class="fx-mobile-sas fx-numeric">{challenge()?.sas}</p>
              <Button variant="primary" onClick={() => void confirm()}>
                {t("mobile.pairConfirm")}
              </Button>
            </div>
          </Show>
        </Show>
        </div>
      </section>


      <PairingQrDialog open={qrOpen()} onOpenChange={setQrOpen} />
      <details class="fx-panel fx-disclosure">
        <summary>{t("devices.trustedTitle")}</summary>
        <TrustedDevices />
      </details>
    </main>
  );
}
