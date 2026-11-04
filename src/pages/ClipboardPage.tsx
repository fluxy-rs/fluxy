import {
  For,
  Show,
  createEffect,
  createSignal,
  onCleanup,
} from "solid-js";
import {
  ArrowDownLeft,
  ArrowUpRight,
  Check,
  Clipboard,
  Clock3,
  History,
  Inbox,
  LoaderCircle,
  LockKeyhole,
  Radio,
  RadioOff,
  Send,
  ShieldCheck,
  X,
} from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { EmptyState } from "../components/EmptyState";
import { Button, toast } from "../ui";
import { formatClock, shortId } from "../lib/format";
import type {
  ClipboardReceivePolicy,
  ClipboardShareView,
} from "../bindings";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

export function ClipboardPage() {
  const engine = useEngine();
  const { t, locale } = useI18n();
  const snapshot = () => engine.snapshot()!;
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [sending, setSending] = createSignal(false);
  const [pendingAction, setPendingAction] = createSignal<string | null>(null);
  const [policyBusy, setPolicyBusy] = createSignal<string | null>(null);
  const [savingAutoSend, setSavingAutoSend] = createSignal(false);
  const [now, setNow] = createSignal(Date.now());

  const policyOptions: {
    value: ClipboardReceivePolicy;
    label: string;
    description: string;
  }[] = [
    {
      value: "ask",
      label: "clipboard.policyAsk",
      description: "clipboard.policyAskBody",
    },
    {
      value: "auto",
      label: "clipboard.policyAuto",
      description: "clipboard.policyAutoBody",
    },
    {
      value: "reject",
      label: "clipboard.policyReject",
      description: "clipboard.policyRejectBody",
    },
  ];

  const timer = window.setInterval(() => setNow(Date.now()), 1000);
  onCleanup(() => window.clearInterval(timer));

  const trusted = () =>
    (snapshot().peers ?? []).filter((peer) => peer.trusted);
  const onlineTrusted = () => trusted().filter((peer) => peer.online);
  const selectedPeer = () =>
    onlineTrusted().find((peer) => peer.device_id === selectedId()) ?? null;
  const pendingIncoming = () =>
    engine
      .clipboardShares()
      .filter(
        (share) =>
          share.direction === "received" &&
          (share.state === "offered" || share.state === "accepted"),
      )
      .sort((a, b) => b.created_at_ms - a.created_at_ms);
  const history = () =>
    engine
      .clipboardShares()
      .filter(
        (share) =>
          !(
            share.direction === "received" &&
            (share.state === "offered" || share.state === "accepted")
          ),
      )
      .sort((a, b) => b.created_at_ms - a.created_at_ms)
      .slice(0, 10);

  createEffect(() => {
    const online = onlineTrusted();
    const current = selectedId();
    if (online.length === 0) {
      if (current) setSelectedId(null);
      return;
    }
    if (!current || !online.some((peer) => peer.device_id === current)) {
      setSelectedId(online[0].device_id);
    }
  });

  createEffect(() => {
    const expired = pendingIncoming().some(
      (share) => share.expires_at_ms <= now(),
    );
    if (expired) void engine.refresh();
  });

  function peerName(deviceId: string): string {
    return (
      trusted().find((peer) => peer.device_id === deviceId)?.name ??
      shortId(deviceId, 16)
    );
  }

  function remaining(share: ClipboardShareView): string {
    if (share.state === "expired") return t("clipboard.expired");
    const seconds = Math.max(
      0,
      Math.ceil((share.expires_at_ms - now()) / 1000),
    );
    return seconds > 0
      ? t("clipboard.expiresIn", { seconds: String(seconds) })
      : t("clipboard.expired");
  }

  async function shareCurrent() {
    const peer = selectedPeer();
    if (!peer || sending()) return;
    setSending(true);
    try {
      await engine.shareClipboard(peer.device_id);
      toast.success(t("clipboard.sentToast", { name: peer.name }));
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("clipboard.sendFailed", { message }));
    } finally {
      setSending(false);
    }
  }

  async function updatePolicy(
    deviceId: string,
    policy: ClipboardReceivePolicy,
  ) {
    if (policyBusy()) return;
    setPolicyBusy(deviceId);
    try {
      await engine.setClipboardPolicy(deviceId, policy);
      toast.success(
        t("clipboard.policySaved", { name: peerName(deviceId) }),
      );
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("clipboard.actionFailed", { message }));
    } finally {
      setPolicyBusy(null);
    }
  }

  async function toggleAutoSend() {
    const next = !snapshot().settings.auto_clipboard;
    setSavingAutoSend(true);
    try {
      await engine.setAutoClipboard(next);
      toast.success(
        next
          ? t("clipboard.autoSendEnabled")
          : t("clipboard.autoSendDisabled"),
      );
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("clipboard.actionFailed", { message }));
    } finally {
      setSavingAutoSend(false);
    }
  }

  async function accept(share: ClipboardShareView) {
    if (pendingAction()) return;
    setPendingAction(share.offer_id);
    try {
      await engine.acceptClipboard(share.device_id, share.offer_id);
      toast.success(
        t("clipboard.acceptedToast", { name: peerName(share.device_id) }),
      );
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("clipboard.actionFailed", { message }));
    } finally {
      setPendingAction(null);
    }
  }

  async function reject(share: ClipboardShareView) {
    if (pendingAction()) return;
    setPendingAction(share.offer_id);
    try {
      await engine.rejectClipboard(share.device_id, share.offer_id);
      toast.success(
        t("clipboard.rejectedToast", { name: peerName(share.device_id) }),
      );
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("clipboard.actionFailed", { message }));
    } finally {
      setPendingAction(null);
    }
  }

  return (
    <div class="fx-page">
      <header class="fx-page-head">
        <div>
          <h1>{t("clipboard.title")}</h1>
          <p>{t("clipboard.subtitle")}</p>
        </div>
        <div class="fx-page-meta">
          <span class="fx-meta-chip">
            <ShieldCheck size={14} aria-hidden="true" />
            {t("app.lanOnly")}
          </span>
          <span class="fx-meta-chip">
            <LockKeyhole size={14} aria-hidden="true" />
            {t("clipboard.policyChip")}
          </span>
        </div>
      </header>

      <div class="fx-two-column">
        <div class="fx-stack">
          <section class="fx-panel fx-panel-main" aria-labelledby="clipboard-send">
            <header class="fx-panel-head">
              <div>
                <h2 id="clipboard-send">{t("clipboard.sendTitle")}</h2>
                <p>{t("clipboard.sendBody")}</p>
              </div>
              <span class="fx-status-pill is-pending">
                {t("clipboard.manualSend")}
              </span>
            </header>
            <Show
              when={onlineTrusted().length > 0}
              fallback={
                <EmptyState
                  icon={<Clipboard size={22} />}
                  title={t("clipboard.noOnlineTitle")}
                  body={t("clipboard.noOnlineBody")}
                  note={t("clipboard.noOnlineNote")}
                />
              }
            >
              <div class="fx-clipboard-peers">
                <p class="fx-clipboard-field-label">
                  {t("clipboard.chooseDevice")}
                </p>
                <ul class="fx-peer-choice-list">
                  <For each={onlineTrusted()}>
                    {(peer) => (
                      <li>
                        <button
                          type="button"
                          class="fx-peer-choice"
                          classList={{
                            "is-selected": selectedId() === peer.device_id,
                          }}
                          onClick={() => setSelectedId(peer.device_id)}
                          aria-pressed={selectedId() === peer.device_id}
                        >
                          <span
                            class="fx-status-dot is-ready"
                            aria-hidden="true"
                          />
                          <span class="fx-peer-choice-main">
                            <strong>{peer.name}</strong>
                            <span>{peer.platform}</span>
                          </span>
                          <Show when={selectedId() === peer.device_id}>
                            <Check
                              size={15}
                              class="fx-peer-choice-check"
                              aria-hidden="true"
                            />
                          </Show>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </div>
              <div class="fx-clipboard-send-row">
                <div class="fx-clipboard-hint">
                  <Clipboard size={16} aria-hidden="true" />
                  <p>{t("clipboard.readHint")}</p>
                </div>
                <Button
                  onClick={() => void shareCurrent()}
                  disabled={!selectedPeer() || sending()}
                  icon={
                    sending() ? (
                      <LoaderCircle size={15} class="fx-spin" />
                    ) : (
                      <Send size={15} />
                    )
                  }
                >
                  {sending()
                    ? t("clipboard.sending")
                    : t("clipboard.sendButton")}
                </Button>
              </div>
            </Show>
          </section>

          <section class="fx-panel" aria-labelledby="clipboard-history">
            <header class="fx-panel-head">
              <div>
                <h2 id="clipboard-history">{t("clipboard.historyTitle")}</h2>
                <p>{t("clipboard.historyBody")}</p>
              </div>
              <History size={15} aria-hidden="true" />
            </header>
            <Show
              when={history().length > 0}
              fallback={
                <p class="fx-note">{t("clipboard.historyEmpty")}</p>
              }
            >
              <ul class="fx-clipboard-history">
                <For each={history()}>
                  {(share) => (
                    <li class="fx-clipboard-history-row">
                      <span
                        class="fx-clipboard-history-icon"
                        aria-hidden="true"
                      >
                        {share.direction === "sent" ? (
                          <ArrowUpRight size={14} />
                        ) : (
                          <ArrowDownLeft size={14} />
                        )}
                      </span>
                      <span class="fx-clipboard-history-main">
                        <strong>
                          {share.direction === "sent"
                            ? t("clipboard.to", {
                                name: peerName(share.device_id),
                              })
                            : t("clipboard.from", {
                                name: peerName(share.device_id),
                              })}
                        </strong>
                        <span class="fx-clipboard-history-preview">
                          {share.preview || t("clipboard.previewEmpty")}
                        </span>
                        <span class="fx-numeric fx-muted">
                          {t(
                            `clipboard.direction.${share.direction}`,
                          )}{" "}
                          · {formatBytes(share.size_bytes)}
                        </span>
                      </span>
                      <span class="fx-clipboard-history-meta">
                        <span
                          class={`fx-state ${share.state}`}
                          title={t(`clipboard.state.${share.state}`)}
                        >
                          {t(`clipboard.state.${share.state}`)}
                        </span>
                        <time class="fx-numeric">
                          {formatClock(share.created_at_ms, locale())}
                        </time>
                      </span>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </section>
        </div>

        <aside class="fx-stack">
          <section class="fx-panel" aria-labelledby="clipboard-incoming">
            <header class="fx-panel-head">
              <div>
                <h2 id="clipboard-incoming">{t("clipboard.incomingTitle")}</h2>
                <p>{t("clipboard.incomingBody")}</p>
              </div>
              <span class="fx-clipboard-count">
                <Inbox size={14} aria-hidden="true" />
                {pendingIncoming().length}
              </span>
            </header>
            <Show
              when={pendingIncoming().length > 0}
              fallback={
                <p class="fx-note">{t("clipboard.incomingEmpty")}</p>
              }
            >
              <ul class="fx-clipboard-incoming">
                <For each={pendingIncoming()}>
                  {(share) => (
                    <li class="fx-clipboard-offer">
                      <div class="fx-clipboard-offer-head">
                        <div>
                          <strong>{peerName(share.device_id)}</strong>
                          <span class="fx-clipboard-meta">
                            <Clock3 size={11} aria-hidden="true" />
                            {remaining(share)}
                          </span>
                        </div>
                        <span
                          class={`fx-state ${share.state}`}
                        >
                          {t(`clipboard.state.${share.state}`)}
                        </span>
                      </div>
                      <p class="fx-clipboard-preview">{share.preview}</p>
                      <div class="fx-clipboard-meta">
                        <span>{formatBytes(share.size_bytes)}</span>
                        <span>
                          {t("clipboard.previewLabel")}
                        </span>
                      </div>
                      <Show
                        when={
                          share.state === "offered" &&
                          share.expires_at_ms > now()
                        }
                      >
                        <div class="fx-clipboard-actions">
                          <Button
                            size="xs"
                            onClick={() => void accept(share)}
                            disabled={pendingAction() === share.offer_id}
                            icon={
                              pendingAction() === share.offer_id ? (
                                <LoaderCircle
                                  size={13}
                                  class="fx-spin"
                                />
                              ) : (
                                <Check size={13} />
                              )
                            }
                          >
                            {t("clipboard.accept")}
                          </Button>
                          <Button
                            size="xs"
                            variant="outline"
                            onClick={() => void reject(share)}
                            disabled={pendingAction() === share.offer_id}
                            icon={<X size={13} />}
                          >
                            {t("clipboard.reject")}
                          </Button>
                        </div>
                      </Show>
                      <Show when={share.state === "accepted"}>
                        <p class="fx-clipboard-waiting">
                          <LoaderCircle size={12} class="fx-spin" />
                          {t("clipboard.waitingText")}
                        </p>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
            <p class="fx-note">{t("clipboard.incomingNote")}</p>
          </section>

          <section class="fx-panel" aria-labelledby="clipboard-policy">
            <header class="fx-panel-head">
              <div>
                <h2 id="clipboard-policy">{t("clipboard.policyTitle")}</h2>
              </div>
              <LockKeyhole size={15} aria-hidden="true" />
            </header>
            <p class="fx-note">{t("clipboard.policyBody")}</p>
            <Show
              when={trusted().length > 0}
              fallback={
                <p class="fx-note">{t("clipboard.policyEmpty")}</p>
              }
            >
              <ul class="fx-policy-list">
                <For each={trusted()}>
                  {(peer) => (
                    <li class="fx-policy-row">
                      <div class="fx-policy-peer">
                        <strong>{peer.name}</strong>
                        <span>
                          {peer.platform} · {shortId(peer.device_id, 12)}
                        </span>
                      </div>
                      <div
                        class="fx-policy-options"
                        role="group"
                        aria-label={t("clipboard.policyFor", {
                          name: peer.name,
                        })}
                      >
                        <For each={policyOptions}>
                          {(option) => (
                            <button
                              type="button"
                              class="fx-policy-option"
                              classList={{
                                "is-active":
                                  peer.clipboard_policy === option.value,
                                "is-auto": option.value === "auto",
                              }}
                              aria-pressed={
                                peer.clipboard_policy === option.value
                              }
                              disabled={policyBusy() !== null}
                              onClick={() =>
                                void updatePolicy(
                                  peer.device_id,
                                  option.value,
                                )
                              }
                            >
                              {t(option.label)}
                            </button>
                          )}
                        </For>
                      </div>
                      <p class="fx-policy-hint">
                        {t(
                          policyOptions.find(
                            (option) =>
                              option.value === peer.clipboard_policy,
                          )?.description ?? "clipboard.policyAskBody",
                        )}
                      </p>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
            <Show
              when={trusted().some(
                (peer) => peer.clipboard_policy === "auto",
              )}
            >
              <p class="fx-policy-warning">
                {t("clipboard.policyWarning")}
              </p>
            </Show>
            <div class="fx-field">
              <button
                class="fx-switch"
                type="button"
                role="switch"
                aria-checked={snapshot().settings.auto_clipboard}
                disabled={savingAutoSend()}
                onClick={() => void toggleAutoSend()}
              >
                <span class="fx-switch-track" aria-hidden="true">
                  <span class="fx-switch-thumb" />
                </span>
                <span class="fx-switch-copy">
                  <strong>{t("clipboard.autoSendLabel")}</strong>
                  <span>
                    {snapshot().settings.auto_clipboard
                      ? t("clipboard.autoSendOn")
                      : t("clipboard.autoSendOff")}
                  </span>
                </span>
                {snapshot().settings.auto_clipboard ? (
                  <Radio size={15} aria-hidden="true" />
                ) : (
                  <RadioOff size={15} aria-hidden="true" />
                )}
              </button>
            </div>
            <Show when={snapshot().settings.auto_clipboard}>
              <p class="fx-note">
                {onlineTrusted().length > 0
                  ? t("clipboard.autoSendTargets", {
                      count: String(onlineTrusted().length),
                    })
                  : t("clipboard.autoSendNoTargets")}
              </p>
              <p class="fx-note">{t("clipboard.autoSendLoopNote")}</p>
            </Show>
            <dl class="fx-definition-list">
              <div>
                <dt>{t("clipboard.sizeLimitLabel")}</dt>
                <dd>{t("clipboard.sizeLimit")}</dd>
              </div>
            </dl>
            <p class="fx-note">{t("clipboard.autoOffNote")}</p>
          </section>
        </aside>
      </div>
    </div>
  );
}
