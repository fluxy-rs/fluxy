import { Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import { Checkbox } from "../ui/checkbox";
import { LoaderCircle, ShieldAlert, ShieldCheck } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog } from "../ui";

function clock(ms: number): string {
  const seconds = Math.ceil(ms / 1000);
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

export function PairingDialog() {
  const engine = useEngine();
  const { t } = useI18n();
  const [acknowledged, setAcknowledged] = createSignal(false);
  const [now, setNow] = createSignal(Date.now());
  let timer: number | undefined;

  const pairing = () => engine.pairing();
  const remaining = () => Math.max(0, (pairing()?.expires_at_ms ?? 0) - now());
  const expired = () => remaining() <= 0;

  createEffect(
    on(
      () => pairing()?.challenge_id,
      () => setAcknowledged(false),
    ),
  );

  onMount(() => {
    timer = window.setInterval(() => setNow(Date.now()), 500);
  });

  onCleanup(() => {
    if (timer !== undefined) window.clearInterval(timer);
  });

  const close = () => {
    void engine.pairCancel();
  };

  return (
    <Show when={pairing()}>
      {(current) => (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open) close();
          }}
          title={t("pairing.dialogTitle")}
          description={
            current().direction === "initiator"
              ? t("pairing.directionInitiator")
              : t("pairing.directionResponder")
          }
          width="md"
          footer={
            <>
              <Button variant="outline" onClick={close} disabled={engine.pairingBusy()}>
                {t("common.cancel")}
              </Button>
              <Button
                onClick={() => void engine.pairConfirm()}
                disabled={!acknowledged() || expired() || engine.pairingBusy()}
                icon={
                  engine.pairingBusy() ? (
                    <LoaderCircle size={14} class="animate-spin" />
                  ) : (
                    <ShieldCheck size={14} />
                  )
                }
              >
                {t("pairing.confirm")}
              </Button>
            </>
          }
        >
          {/* One line, not two stacked cards: the pairing code is the thing to
              compare, and the two device names only need to say who is talking to
              whom. Two cards spent half a phone screen on four short strings. */}
          <p class="flex items-center gap-2 text-sm">
            <strong>{engine.snapshot()?.status.device_name}</strong>
            <span class="text-muted-foreground" aria-hidden="true">
              ↔
            </span>
            <strong title={current().peer_name}>{current().peer_name}</strong>
            <span class="text-xs text-muted-foreground">{current().peer_platform}</span>
          </p>

          <div class="flex flex-col items-center gap-1 rounded-lg bg-muted px-4 py-3">
            <span class="text-label text-muted-foreground">{t("pairing.sasLabel")}</span>
            <output
              class="font-mono text-lg tracking-[0.3em]"
              aria-label={t("pairing.sasLabel")}
              aria-live="polite"
              aria-atomic="true"
            >
              <span>{current().sas.slice(0, 3)}</span>
              <i aria-hidden="true" />
              <span>{current().sas.slice(3)}</span>
            </output>
            <span class="text-label text-muted-foreground" classList={{ "is-expired": expired() }}>
              {expired()
                ? t("pairing.expired")
                : t("pairing.expiresIn", { time: clock(remaining()) })}
            </span>
          </div>

          <div class="rounded-lg bg-warning/10 px-3 py-2 text-xs text-warning">
            <ShieldAlert size={17} aria-hidden="true" />
            <p class="text-body-sm">{t("pairing.warning")}</p>
          </div>

          <label class="flex items-center gap-2">
            <Checkbox
              checked={acknowledged()}
              disabled={expired() || engine.pairingBusy()}
              onChange={(checked) => setAcknowledged(checked)}
            />
            <span>{t("pairing.acknowledge")}</span>
          </label>

          <Show when={engine.pairingError()}>
            {(error) => (
              <p class="text-xs text-destructive" role="alert">
                {t("pairing.requestFailed", { message: error().message })}
              </p>
            )}
          </Show>
          <p class="text-xs text-muted-foreground">{t("pairing.noAuto")}</p>
        </Dialog>
      )}
    </Show>
  );
}
