import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { Check, Copy, LoaderCircle, Network, RotateCcw, ShieldAlert, Wifi } from "lucide-solid";
import type { InterfaceOptionView } from "../bindings";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, toast } from "../ui";
import { copyText, formatTime } from "../lib/format";

type Mode = "auto" | "manual";

/** Platform interface classes, as the backend names them. */
const KIND_KEYS: Record<string, string> = {
  ethernet: "network.kindEthernet",
  wireless: "network.kindWireless",
  tunnel: "network.kindTunnel",
  loopback: "network.kindLoopback",
  point_to_point: "network.kindPointToPoint",
  other: "network.kindOther",
};

/** Why the picker chose the active address. */
const SOURCE_KEYS: Record<string, string> = {
  default_route: "network.sourceDefaultRoute",
  private_address: "network.sourcePrivateAddress",
  first_available: "network.sourceFirstAvailable",
  loopback: "network.sourceLoopback",
  manual: "network.sourceManual",
};

/**
 * Interface selection and its diagnostics. The runtime owns the actual binding;
 * this panel proposes a selection, explains what the runtime chose, and turns a
 * failure into an actionable next step. Switching drops sessions, so every apply
 * goes through a confirmation that says so.
 */
export function NetworkSettings() {
  const engine = useEngine();
  const { t, locale } = useI18n();
  const network = () => engine.snapshot()!.network;

  const [mode, setMode] = createSignal<Mode>("auto");
  const [interfaceName, setInterfaceName] = createSignal<string | null>(null);
  const [address, setAddress] = createSignal<string | null>(null);
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  const [applying, setApplying] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);
  let applyTrigger: HTMLButtonElement | undefined;

  const isDown = () => network().state === "unavailable" || network().state === "stopped";

  /** The interface the runtime is bound to, matched by platform index. */
  const activeOption = createMemo(() => {
    const options = network().interface_options;
    const index = network().interface_index;
    return (
      options.find((option) => index !== null && option.index === index) ??
      options.find((option) => option.recommended) ??
      null
    );
  });

  const selectedOption = createMemo(() => {
    const name = interfaceName();
    return network().interface_options.find((option) => option.name === name) ?? null;
  });

  /** True while the panel proposes something the runtime has not applied. */
  const dirty = createMemo(() => {
    if (mode() !== network().selection_mode) return true;
    if (mode() === "auto") return false;
    const option = selectedOption();
    if (!option) return false;
    if (option.index !== network().interface_index) return true;
    const chosen = address();
    return chosen !== null && chosen !== network().local_address;
  });

  function seed() {
    setMode(network().selection_mode === "manual" ? "manual" : "auto");
    const option = activeOption();
    setInterfaceName(option?.name ?? null);
    setAddress(network().local_address ?? option?.addresses[0] ?? null);
  }

  seed();

  // Adopt a selection the runtime moved on its own (an automatic rebind, or a
  // manual address that came back) unless the user has an unapplied change.
  createEffect(
    on(
      () =>
        network().selection_mode +
        "|" +
        String(network().interface_index) +
        "|" +
        (network().local_address ?? ""),
      () => {
        if (!dirty()) seed();
      },
    ),
  );

  function chooseInterface(option: InterfaceOptionView) {
    setInterfaceName(option.name);
    setAddress(option.addresses[0] ?? null);
    setFailure(null);
  }

  function chooseMode(next: Mode) {
    setMode(next);
    setFailure(null);
    if (next === "manual" && !selectedOption()) {
      const option = activeOption();
      if (option) {
        setInterfaceName(option.name);
        setAddress(option.addresses[0] ?? null);
      }
    }
  }

  async function apply() {
    setApplying(true);
    setFailure(null);
    try {
      await engine.setNetworkSelection(
        mode(),
        mode() === "manual" ? interfaceName() : null,
        mode() === "manual" ? address() : null,
      );
      toast.success(t("network.switchApplied"));
      setConfirmOpen(false);
      seed();
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      // Keep the panel in the state the user chose: the runtime did not move, so
      // the proposed selection is still the fix they can retry or adjust.
      setFailure(message);
      toast.error(t("network.switchFailed", { message }));
      setConfirmOpen(false);
    } finally {
      setApplying(false);
    }
  }

  function interfaceText(): string {
    const view = network();
    const lines = view.interface_options.map((option) => {
      const marks = [
        option.up ? "up" : "down",
        option.kind,
        "index=" + String(option.index),
        option.virtual_adapter ? "virtual" : "physical",
        option.default_route ? "default-route" : "no-default-route",
        option.recommended ? "recommended" : "",
        option.index === view.interface_index ? "active" : "",
      ].filter(Boolean);
      return (
        option.label +
        " (" +
        option.name +
        ") [" +
        marks.join(", ") +
        "] addresses=" +
        (option.addresses.join(", ") || "none")
      );
    });
    return [
      "mode=" + view.selection_mode,
      "source=" + view.selection_source,
      "interface=" + (view.interface ?? "-"),
      "address=" + (view.local_address ?? "-"),
      "listen=" + (view.listen_addr ?? "-"),
      "pinned=" + String(view.interface_pinned),
      "rebinds=" + String(view.rebind_count),
      "",
      ...lines,
    ].join("\n");
  }

  async function copyInterfaces() {
    if (await copyText(interfaceText())) {
      toast.success(t("network.copiedInterfaces"));
    } else {
      toast.error(t("common.copyFailed"));
    }
  }

  function rebindText(): string {
    if (network().rebind_count === 0) return t("network.rebindNever");
    const base = t("network.rebindCount", { count: network().rebind_count });
    const last = network().last_rebind_ms;
    if (last === null) return base;
    return base + " · " + t("network.rebindLast", { time: formatTime(last, locale()) });
  }

  const kindKey = (kind: string) => KIND_KEYS[kind] ?? "network.kindOther";
  const sourceKey = (source: string) => SOURCE_KEYS[source] ?? "network.sourceFirstAvailable";
  const isCurrent = (option: InterfaceOptionView) => option.index === network().interface_index;
  const isSelected = (option: InterfaceOptionView) =>
    mode() === "manual" && option.name === interfaceName();

  return (
    <section
      class="overflow-hidden rounded-lg border border-border bg-card"
      aria-labelledby="network-settings-heading"
    >
      <header class="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div>
          <h2 id="network-settings-heading" class="text-title font-[620] tracking-[-0.012em]">
            {t("network.selectionTitle")}
          </h2>
          <p class="text-body-sm text-muted-foreground">{t("network.selectionBody")}</p>
        </div>
        <Button
          variant="outline"
          size="xs"
          onClick={() => void copyInterfaces()}
          icon={<Copy size={13} />}
        >
          {t("network.copyInterfaces")}
        </Button>
      </header>

      <div class="flex items-center gap-2 border-b border-border bg-muted/40 px-4 py-2 text-xs">
        <span
          class={`rounded-full border px-2 py-0.5 text-label ${
            isDown()
              ? "border-destructive/40 bg-destructive/10 text-destructive"
              : "border-success/40 bg-success/10 text-success"
          }`}
        >
          <Network size={13} aria-hidden="true" />
          {network().interface ?? t("network.none")}
        </span>
        <span class="px-4 py-3 text-xs text-muted-foreground">
          {network().local_address ?? t("network.none")}
          {" · "}
          {t(sourceKey(network().selection_source))}
          {" · "}
          {network().interface_pinned ? t("network.pinnedNote") : t("network.unpinnedNote")}
        </span>
      </div>

      <Show when={isDown() || failure()}>
        <div class="rounded-lg bg-warning/10 px-3 py-2 text-xs text-warning" role="alert">
          <ShieldAlert size={16} aria-hidden="true" />
          <div>
            <strong>{t("network.guidanceTitle")}</strong>
            <p class="text-body-sm">
              {network().selection_mode === "manual"
                ? t("network.guidanceManualLost")
                : t("network.guidanceUnavailable")}
            </p>
            <p class="text-body-sm text-muted-foreground">{t("network.guidancePick")}</p>
            <Show when={failure()}>
              {(message) => (
                <p class="text-body-sm text-destructive">
                  {t("network.guidanceError", { message: message() })}
                </p>
              )}
            </Show>
            <Show when={network().detail}>
              {(detail) => <p class="px-4 py-3 text-xs text-muted-foreground">{detail()}</p>}
            </Show>
          </div>
        </div>
      </Show>

      <div class="flex flex-col gap-2 px-4 py-3">
        <span class="text-sm font-medium">{t("network.modeLabel")}</span>
        <div class="flex gap-1" role="group" aria-label={t("network.modeLabel")}>
          <button
            class="rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
            classList={{ "is-selected": mode() === "auto" }}
            type="button"
            aria-pressed={mode() === "auto"}
            onClick={() => chooseMode("auto")}
          >
            <Network size={15} aria-hidden="true" />
            <span>{t("network.modeAuto")}</span>
            <Show when={mode() === "auto"}>
              <Check size={14} aria-hidden="true" />
            </Show>
          </button>
          <button
            class="rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
            classList={{ "is-selected": mode() === "manual" }}
            type="button"
            aria-pressed={mode() === "manual"}
            onClick={() => chooseMode("manual")}
          >
            <Wifi size={15} aria-hidden="true" />
            <span>{t("network.modeManual")}</span>
            <Show when={mode() === "manual"}>
              <Check size={14} aria-hidden="true" />
            </Show>
          </button>
        </div>
        <p class="text-xs text-muted-foreground">
          {mode() === "auto" ? t("network.modeAutoHint") : t("network.modeManualHint")}
        </p>
      </div>

      <div class="flex flex-col gap-2 px-4 py-3">
        <span class="text-sm font-medium">{t("network.interfaceList")}</span>
        <ul class="flex flex-col gap-2 px-4 py-3">
          <For each={network().interface_options}>
            {(option) => (
              <li>
                <button
                  type="button"
                  class="flex flex-col gap-1 rounded-lg border border-border p-3 text-left transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-accent"
                  classList={{
                    "is-selected": isSelected(option),
                    "is-current": isCurrent(option),
                  }}
                  aria-pressed={isSelected(option)}
                  disabled={mode() !== "manual" || !option.up}
                  title={t("network.interfaceSelect", { label: option.label })}
                  onClick={() => chooseInterface(option)}
                >
                  <span class="flex items-center gap-2">
                    <strong>{option.label}</strong>
                    <span class="rounded-full bg-muted px-2 py-0.5 text-label text-muted-foreground">
                      {t(kindKey(option.kind))}
                    </span>
                    <Show when={isCurrent(option)}>
                      <span class="rounded-full border border-border px-2 py-0.5 text-label text-muted-foreground">
                        {t("network.interfaceCurrent")}
                      </span>
                    </Show>
                    <Show when={option.recommended}>
                      <span class="rounded-full border border-border px-2 py-0.5 text-label text-muted-foreground">
                        {t("network.interfaceRecommended")}
                      </span>
                    </Show>
                    <Show when={option.default_route}>
                      <span class="rounded-full border border-border px-2 py-0.5 text-label text-muted-foreground">
                        {t("network.interfaceDefaultRoute")}
                      </span>
                    </Show>
                    <Show when={option.virtual_adapter}>
                      <span class="rounded-full bg-warning/10 px-2 py-0.5 text-label text-warning">
                        {t("network.interfaceVirtual")}
                      </span>
                    </Show>
                    <Show when={!option.up}>
                      <span class="rounded-full bg-warning/10 px-2 py-0.5 text-label text-warning">
                        {t("network.interfaceDown")}
                      </span>
                    </Show>
                    <span class="ml-auto font-mono text-label text-muted-foreground">
                      {t("network.interfaceIndex", { index: option.index })}
                    </span>
                  </span>
                  <span class="flex flex-col gap-0.5 font-mono text-xs text-muted-foreground">
                    <For
                      each={option.addresses}
                      fallback={
                        <span class="text-xs text-muted-foreground">{t("network.none")}</span>
                      }
                    >
                      {(value) => (
                        <span class="font-mono text-xs">
                          {value}
                          <Show when={value.includes("%")}> {t("network.addressLinkLocal")}</Show>
                        </span>
                      )}
                    </For>
                  </span>
                </button>
              </li>
            )}
          </For>
        </ul>
        <p class="text-xs text-muted-foreground">{t("network.interfaceListHint")}</p>
      </div>

      <Show when={mode() === "manual" && selectedOption()}>
        {(option) => (
          <div class="flex flex-col gap-2 px-4 py-3">
            <span class="text-sm font-medium">{t("network.addressLabel")}</span>
            <Show
              when={option().addresses.length > 1}
              fallback={<p class="font-mono text-xs">{address() ?? t("network.none")}</p>}
            >
              <div class="flex gap-1" role="group" aria-label={t("network.addressLabel")}>
                <For each={option().addresses}>
                  {(value, index) => (
                    <button
                      class="rounded-lg border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground"
                      classList={{ "is-selected": address() === value }}
                      type="button"
                      aria-pressed={address() === value}
                      onClick={() => {
                        setAddress(value);
                        setFailure(null);
                      }}
                    >
                      <span class="font-mono text-xs">{value}</span>
                      <Show when={index() === 0}>
                        <span class="rounded-full border border-border px-2 py-0.5 text-label text-muted-foreground">
                          {t("network.addressPreferred")}
                        </span>
                      </Show>
                      <Show when={address() === value}>
                        <Check size={14} aria-hidden="true" />
                      </Show>
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </div>
        )}
      </Show>

      <dl class="grid grid-cols-[10rem_minmax(0,1fr)] gap-x-4 gap-y-2 px-4 py-3 text-sm">
        <div>
          <dt>{t("network.listenAddr")}</dt>
          <dd class="font-mono text-xs">{network().listen_addr ?? t("network.none")}</dd>
        </div>
        <div>
          <dt>{t("network.rebindLast")}</dt>
          <dd class="font-mono text-xs">{rebindText()}</dd>
        </div>
      </dl>

      <div class="flex items-center gap-2 px-4 py-3">
        <Show when={dirty()}>
          <span class="rounded-full bg-warning/10 px-2 py-0.5 text-label text-warning">
            {t("network.pendingChange")}
          </span>
        </Show>
        <Button
          variant="outline"
          onClick={() => {
            seed();
            setFailure(null);
          }}
          disabled={!dirty()}
          icon={<RotateCcw size={14} />}
        >
          {t("network.revert")}
        </Button>
        <Button
          ref={(element) => {
            applyTrigger = element;
          }}
          disabled={!dirty() || applying()}
          onClick={() => setConfirmOpen(true)}
          icon={applying() ? <LoaderCircle size={14} class="animate-spin" /> : <Check size={14} />}
        >
          {t("network.apply")}
        </Button>
      </div>

      <Dialog
        open={confirmOpen()}
        onOpenChange={setConfirmOpen}
        title={t("network.confirmTitle")}
        description={t("network.confirmBody")}
        width="sm"
        returnFocus={applyTrigger}
        footer={
          <>
            <Button variant="outline" onClick={() => setConfirmOpen(false)} disabled={applying()}>
              {t("common.cancel")}
            </Button>
            <Button
              onClick={() => void apply()}
              disabled={applying()}
              icon={
                applying() ? <LoaderCircle size={14} class="animate-spin" /> : <Check size={14} />
              }
            >
              {t("network.confirmAction")}
            </Button>
          </>
        }
      >
        <p class="text-xs text-destructive">{t("network.confirmWarning")}</p>
      </Dialog>
    </section>
  );
}
