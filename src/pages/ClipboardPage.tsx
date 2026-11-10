import { platformLabel } from "../lib/platform";
import { For, Show, createEffect, createSignal, onCleanup } from "solid-js";
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
  Send,
  ShieldCheck,
  X,
} from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { EmptyState } from "../components/EmptyState";
import { PageFrame } from "../components/PageFrame";
import { PageTitle } from "../components/PageTitle";
import { Button, toast } from "../ui";
import { Switch } from "../ui/switch";
import { formatClock, shortId } from "../lib/format";
import type { ClipboardReceivePolicy, ClipboardShareView } from "../bindings";

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

  const trusted = () => (snapshot().peers ?? []).filter((peer) => peer.trusted);
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
    const expired = pendingIncoming().some((share) => share.expires_at_ms <= now());
    if (expired) void engine.refresh();
  });

  function peerName(deviceId: string): string {
    return trusted().find((peer) => peer.device_id === deviceId)?.name ?? shortId(deviceId, 16);
  }

  function remaining(share: ClipboardShareView): string {
    if (share.state === "expired") return t("clipboard.expired");
    const seconds = Math.max(0, Math.ceil((share.expires_at_ms - now()) / 1000));
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

  async function updatePolicy(deviceId: string, policy: ClipboardReceivePolicy) {
    if (policyBusy()) return;
    setPolicyBusy(deviceId);
    try {
      await engine.setClipboardPolicy(deviceId, policy);
      toast.success(t("clipboard.policySaved", { name: peerName(deviceId) }));
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
      toast.success(next ? t("clipboard.autoSendEnabled") : t("clipboard.autoSendDisabled"));
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
      toast.success(t("clipboard.acceptedToast", { name: peerName(share.device_id) }));
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
      toast.success(t("clipboard.rejectedToast", { name: peerName(share.device_id) }));
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      toast.error(t("clipboard.actionFailed", { message }));
    } finally {
      setPendingAction(null);
    }
  }

  // A template literal in class= is invisible to Tailwind's scanner, so the
  // state colours have to be written out.
  const stateChipClass = (state: string) => {
    const base = "rounded-full px-2 py-0.5 text-label";
    if (state === "delivered") return `${base} bg-success/10 text-success`;
    if (state === "expired") return `${base} bg-muted text-muted-foreground`;
    return `${base} bg-warning/10 text-warning`;
  };

  return (
    <PageFrame>
      <PageTitle
        title={t("clipboard.title")}
        subtitle={t("clipboard.subtitle")}
        aside={
          <>
            <span class="flex items-center gap-1.5 rounded-full border border-border px-2.5 py-1 text-label text-muted-foreground">
              <ShieldCheck size={13} aria-hidden="true" />
              {t("app.lanOnly")}
            </span>
            <span class="flex items-center gap-1.5 rounded-full border border-border px-2.5 py-1 text-label text-muted-foreground">
              <LockKeyhole size={13} aria-hidden="true" />
              {t("clipboard.policyChip")}
            </span>
          </>
        }
      />

      <div class="grid grid-cols-1 gap-4 lg:grid-cols-2 lg:items-start">
        <div class="flex flex-col gap-4">
          <section
            class="overflow-hidden rounded-lg border border-border bg-card"
            aria-labelledby="clipboard-send"
          >
            <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
              <div>
                <h2 id="clipboard-send">{t("clipboard.sendTitle")}</h2>
                <p class="text-body-sm text-muted-foreground">{t("clipboard.sendBody")}</p>
              </div>
              <span class="rounded-full border border-border px-2 py-0.5 text-label text-muted-foreground">
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
              <div class="flex flex-col gap-2 px-4 py-3">
                <p class="text-xs font-medium text-muted-foreground">
                  {t("clipboard.chooseDevice")}
                </p>
                <ul class="grid grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] gap-2">
                  <For each={onlineTrusted()}>
                    {(peer) => (
                      <li>
                        <button
                          type="button"
                          class="flex w-full items-center gap-2.5 rounded-lg border border-border px-3 py-2 text-left transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-accent"
                          onClick={() => setSelectedId(peer.device_id)}
                          aria-pressed={selectedId() === peer.device_id}
                        >
                          <span
                            class="size-2 shrink-0 rounded-full bg-success"
                            aria-hidden="true"
                          />
                          <span class="flex min-w-0 flex-1 items-center gap-1.5">
                            <strong class="truncate text-body font-semibold">{peer.name}</strong>
                            <span class="shrink-0 text-micro text-muted-foreground">
                              {platformLabel(peer.platform)}
                            </span>
                          </span>
                          <Show when={selectedId() === peer.device_id}>
                            <Check size={15} class="shrink-0 text-primary" aria-hidden="true" />
                          </Show>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </div>
              <div class="flex items-center justify-between gap-3 border-t border-border px-4 py-3">
                <div class="text-xs text-muted-foreground">
                  <Clipboard size={16} aria-hidden="true" />
                  <p class="text-body-sm">{t("clipboard.readHint")}</p>
                </div>
                <Button
                  onClick={() => void shareCurrent()}
                  disabled={!selectedPeer() || sending()}
                  icon={
                    sending() ? <LoaderCircle size={15} class="animate-spin" /> : <Send size={15} />
                  }
                >
                  {sending() ? t("clipboard.sending") : t("clipboard.sendButton")}
                </Button>
              </div>
            </Show>
          </section>

          <section
            class="overflow-hidden rounded-lg border border-border bg-card"
            aria-labelledby="clipboard-history"
          >
            <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
              <div>
                <h2 id="clipboard-history">{t("clipboard.historyTitle")}</h2>
                <p class="text-body-sm text-muted-foreground">{t("clipboard.historyBody")}</p>
              </div>
              <History size={15} aria-hidden="true" />
            </header>
            <Show
              when={history().length > 0}
              fallback={
                <p class="px-4 py-3 text-xs text-muted-foreground">{t("clipboard.historyEmpty")}</p>
              }
            >
              <ul class="flex flex-col divide-y divide-border">
                <For each={history()}>
                  {(share) => (
                    <li class="flex items-start gap-2.5 px-4 py-2.5">
                      <span class="mt-0.5 shrink-0 text-muted-foreground" aria-hidden="true">
                        {share.direction === "sent" ? (
                          <ArrowUpRight size={14} />
                        ) : (
                          <ArrowDownLeft size={14} />
                        )}
                      </span>
                      <span class="flex min-w-0 flex-1 flex-col gap-0.5">
                        <strong>
                          {share.direction === "sent"
                            ? t("clipboard.to", {
                                name: peerName(share.device_id),
                              })
                            : t("clipboard.from", {
                                name: peerName(share.device_id),
                              })}
                        </strong>
                        <span class="truncate text-sm">
                          {share.preview || t("clipboard.previewEmpty")}
                        </span>
                        <span class="font-mono text-label text-muted-foreground">
                          {t(`clipboard.direction.${share.direction}`)} ·{" "}
                          {formatBytes(share.size_bytes)}
                        </span>
                      </span>
                      <span class="flex shrink-0 items-center gap-2 text-label text-muted-foreground">
                        <span
                          class={stateChipClass(share.state)}
                          title={t(`clipboard.state.${share.state}`)}
                        >
                          {t(`clipboard.state.${share.state}`)}
                        </span>
                        <time class="font-mono text-label">
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

        <aside class="flex flex-col gap-4">
          <section
            class="overflow-hidden rounded-lg border border-border bg-card"
            aria-labelledby="clipboard-incoming"
          >
            <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
              <div class="min-w-0">
                <h2 id="clipboard-incoming">{t("clipboard.incomingTitle")}</h2>
                <p class="text-body-sm text-muted-foreground">{t("clipboard.incomingBody")}</p>
              </div>
              <span class="flex shrink-0 items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                <Inbox size={14} aria-hidden="true" />
                {pendingIncoming().length}
              </span>
            </header>
            <Show
              when={pendingIncoming().length > 0}
              fallback={
                <p class="px-4 py-3 text-xs text-muted-foreground">
                  {t("clipboard.incomingEmpty")}
                </p>
              }
            >
              <ul class="flex flex-col divide-y divide-border">
                <For each={pendingIncoming()}>
                  {(share) => (
                    <li class="flex flex-col gap-2 px-4 py-3">
                      <div class="flex items-center justify-between gap-2">
                        <div>
                          <strong>{peerName(share.device_id)}</strong>
                          <span class="flex items-center gap-2 text-label text-muted-foreground">
                            <Clock3 size={11} aria-hidden="true" />
                            {remaining(share)}
                          </span>
                        </div>
                        <span class={stateChipClass(share.state)}>
                          {t(`clipboard.state.${share.state}`)}
                        </span>
                      </div>
                      <p class="line-clamp-3 text-sm text-muted-foreground">{share.preview}</p>
                      <div class="flex items-center gap-2 text-label text-muted-foreground">
                        <span>{formatBytes(share.size_bytes)}</span>
                        <span>{t("clipboard.previewLabel")}</span>
                      </div>
                      <Show when={share.state === "offered" && share.expires_at_ms > now()}>
                        <div class="flex items-center gap-2">
                          <Button
                            size="xs"
                            onClick={() => void accept(share)}
                            disabled={pendingAction() === share.offer_id}
                            icon={
                              pendingAction() === share.offer_id ? (
                                <LoaderCircle size={13} class="animate-spin" />
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
                        <p class="flex items-center gap-2 px-4 py-3 text-xs text-muted-foreground">
                          <LoaderCircle size={12} class="animate-spin" />
                          {t("clipboard.waitingText")}
                        </p>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
            <p class="px-4 py-3 text-xs text-muted-foreground">{t("clipboard.incomingNote")}</p>
          </section>

          <section
            class="overflow-hidden rounded-lg border border-border bg-card"
            aria-labelledby="clipboard-policy"
          >
            <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
              <div>
                <h2 id="clipboard-policy">{t("clipboard.policyTitle")}</h2>
              </div>
              <LockKeyhole size={15} aria-hidden="true" />
            </header>
            <p class="px-4 py-3 text-xs text-muted-foreground">{t("clipboard.policyBody")}</p>
            <Show
              when={trusted().length > 0}
              fallback={
                <p class="px-4 py-3 text-xs text-muted-foreground">{t("clipboard.policyEmpty")}</p>
              }
            >
              <ul class="flex flex-col divide-y divide-border">
                <For each={trusted()}>
                  {(peer) => (
                    <li class="flex flex-col gap-2 px-4 py-3">
                      <div class="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
                        <strong class="text-body font-semibold">{peer.name}</strong>
                        <span class="text-micro text-muted-foreground">
                          {platformLabel(peer.platform)}
                        </span>
                        <span class="font-mono text-micro text-muted-foreground">
                          {shortId(peer.device_id, 12)}
                        </span>
                      </div>
                      <div
                        class="flex flex-wrap gap-1"
                        role="group"
                        aria-label={t("clipboard.policyFor", {
                          name: peer.name,
                        })}
                      >
                        <For each={policyOptions}>
                          {(option) => (
                            <button
                              type="button"
                              class="rounded-lg border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
                              classList={{
                                "is-active": peer.clipboard_policy === option.value,
                                "is-auto": option.value === "auto",
                              }}
                              aria-pressed={peer.clipboard_policy === option.value}
                              disabled={policyBusy() !== null}
                              onClick={() => void updatePolicy(peer.device_id, option.value)}
                            >
                              {t(option.label)}
                            </button>
                          )}
                        </For>
                      </div>
                      <p class="text-label text-muted-foreground">
                        {t(
                          policyOptions.find((option) => option.value === peer.clipboard_policy)
                            ?.description ?? "clipboard.policyAskBody",
                        )}
                      </p>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
            <Show when={trusted().some((peer) => peer.clipboard_policy === "auto")}>
              <p class="rounded-lg bg-warning/10 px-2.5 py-1.5 text-label text-warning">
                {t("clipboard.policyWarning")}
              </p>
            </Show>
            <div class="border-t border-border px-4 py-3">
              <div class="flex items-center justify-between gap-3">
                <span class="flex min-w-0 flex-col gap-0.5">
                  <strong class="text-sm">{t("clipboard.autoSendLabel")}</strong>
                  <span class="text-xs text-muted-foreground">
                    {snapshot().settings.auto_clipboard
                      ? t("clipboard.autoSendOn")
                      : t("clipboard.autoSendOff")}
                  </span>
                </span>
                <Switch
                  checked={snapshot().settings.auto_clipboard}
                  disabled={savingAutoSend()}
                  aria-label={t("clipboard.autoSendLabel")}
                  onCheckedChange={() => void toggleAutoSend()}
                />
              </div>
            </div>
            <Show when={snapshot().settings.auto_clipboard}>
              <p class="px-4 py-3 text-xs text-muted-foreground">
                {onlineTrusted().length > 0
                  ? t("clipboard.autoSendTargets", {
                      count: String(onlineTrusted().length),
                    })
                  : t("clipboard.autoSendNoTargets")}
              </p>
              <p class="px-4 py-3 text-xs text-muted-foreground">
                {t("clipboard.autoSendLoopNote")}
              </p>
            </Show>
            <dl class="grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
              <div>
                <dt>{t("clipboard.sizeLimitLabel")}</dt>
                <dd>{t("clipboard.sizeLimit")}</dd>
              </div>
            </dl>
            <p class="px-4 py-3 text-xs text-muted-foreground">{t("clipboard.autoOffNote")}</p>
          </section>
        </aside>
      </div>
    </PageFrame>
  );
}
