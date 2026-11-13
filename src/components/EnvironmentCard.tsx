import { For, createMemo, createSignal } from "solid-js";
import { Button, toast } from "../ui";
import { useI18n } from "../i18n";
import { useEngine } from "../state/engine";

/**
 * What this machine is running.
 *
 * Added after a car head unit showed the app with no styles at all and there
 * was no debugger to ask: the answer is almost always the WebView version, and
 * the only way to get it out of such a device is to print it on screen. The
 * probes say which CSS the engine understands, which is what decides whether a
 * bundle renders or falls apart.
 */
export function EnvironmentCard() {
  const engine = useEngine();
  const { t } = useI18n();
  const [copied, setCopied] = createSignal(false);

  const supports = (feature: string) => (CSS.supports(feature) ? "✓" : "✗");

  /// Cascade layers cannot be asked about with CSS.supports - the first version
  /// here tested `(color: red)` and reported a cheerful tick on an engine that
  /// drops every layer. A one-rule sheet is the only honest probe.
  const supportsLayers = createMemo(() => {
    const style = document.createElement("style");
    style.textContent = "@layer fx-probe{.fx-probe{color:rgb(1,2,3)}}";
    document.head.append(style);
    const probe = document.createElement("div");
    probe.className = "fx-probe";
    document.body.append(probe);
    const layered = getComputedStyle(probe).color === "rgb(1, 2, 3)";
    probe.remove();
    style.remove();
    return layered;
  });

  const rows = createMemo(() => [
    { label: t("help.envVersion"), value: engine.snapshot()?.status.version ?? "?" },
    { label: t("help.envPlatform"), value: navigator.platform || "?" },
    { label: t("help.envAgent"), value: navigator.userAgent },
    { label: t("help.envLayers"), value: (supportsLayers() ? "✓" : "✗") + " @layer" },
    { label: t("help.envColors"), value: supports("color: oklch(0.5 0.1 200)") + " oklch" },
    {
      label: t("help.envMix"),
      value: supports("color: color-mix(in srgb, red, blue)") + " color-mix",
    },
    {
      label: t("help.envViewport"),
      value: `${window.innerWidth}×${window.innerHeight} @${window.devicePixelRatio}`,
    },
  ]);

  const report = () =>
    rows()
      .map((row) => `${row.label}: ${row.value}`)
      .join("\n");

  async function copy() {
    try {
      await navigator.clipboard.writeText(report());
      setCopied(true);
      toast.success(t("help.envCopied"));
    } catch {
      toast.error(t("chat.copyFailed"));
    }
  }

  return (
    <section class="rounded-lg border border-border bg-card p-4">
      <header class="mb-2 flex items-center justify-between gap-2">
        <div>
          <h2 class="text-title font-[620]">{t("help.envTitle")}</h2>
          <p class="text-body-sm text-muted-foreground">{t("help.envBody")}</p>
        </div>
        <Button size="sm" variant="outline" onClick={() => void copy()}>
          {copied() ? t("help.envCopied") : t("help.envCopy")}
        </Button>
      </header>
      <dl class="grid gap-1.5 text-body-sm">
        <For each={rows()}>
          {(row) => (
            <div class="grid grid-cols-[10rem_minmax(0,1fr)] gap-2">
              <dt class="text-muted-foreground">{row.label}</dt>
              <dd class="break-all font-mono text-xs">{row.value}</dd>
            </div>
          )}
        </For>
      </dl>
    </section>
  );
}
