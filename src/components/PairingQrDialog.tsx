import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import { renderSVG } from "uqr";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { pairingPayload } from "../lib/pairing";
import { Dialog } from "../ui";

/**
 * "Show my code" - the half of pairing a desktop needs.
 *
 * Scanning is a phone action: a desktop usually has no camera and no way to aim
 * one. But the desktop is exactly the device a phone wants to scan, so this
 * component lives on both surfaces and either side can be the one being read.
 */
export function PairingQrDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const engine = useEngine();
  const { t } = useI18n();
  const [svg, setSvg] = createSignal("");

  const refresh = () => {
    const payload = pairingPayload(engine.snapshot());
    if (!payload) {
      setSvg("");
      return;
    }
    try {
      // renderSVG is synchronous and returns a complete <svg> string. The
      // previous library mutated Object.prototype.toString on import and
      // crashed the whole bundle under strict mode.
      setSvg(renderSVG(payload, { border: 2 }));
    } catch {
      setSvg("");
    }
  };

  // The address follows the network, so the code is rebuilt when the dialog
  // opens and refreshed while it stays open rather than once at mount.
  createEffect(() => {
    if (props.open) refresh();
  });
  const timer = window.setInterval(() => {
    if (props.open) refresh();
  }, 4000);
  onCleanup(() => window.clearInterval(timer));

  const status = () => engine.snapshot()?.status;
  const address = () => {
    const snapshot = engine.snapshot();
    const host = snapshot?.network?.local_address ?? "";
    return host ? `${host}:${status()?.listen_port ?? 0}` : "";
  };

  return (
    <Dialog
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={t("pairing.myQrTitle")}
      description={t("pairing.myQrBody")}
      width="sm"
    >
      <Show
        when={svg()}
        fallback={<p class="fx-muted">{t("pairing.myQrUnavailable")}</p>}
      >
        <div class="fx-qr" innerHTML={svg()} />
      </Show>
      <p class="fx-note fx-numeric fx-qr-address">
        {status()?.device_name} · {address()}
      </p>
    </Dialog>
  );
}
