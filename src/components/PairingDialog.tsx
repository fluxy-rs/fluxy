import { Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
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
                disabled={
                  !acknowledged() ||
                  expired() ||
                  engine.pairingBusy()
                }
                icon={
                  engine.pairingBusy() ? (
                    <LoaderCircle size={14} class="fx-spin" />
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
          <p class="fx-pair-pairline">
            <strong>{engine.snapshot()?.status.device_name}</strong>
            <span class="fx-pair-arrow" aria-hidden="true">
              ↔
            </span>
            <strong title={current().peer_name}>{current().peer_name}</strong>
            <span class="fx-pair-platform">{current().peer_platform}</span>
          </p>

          <div class="fx-sas-wrap">
            <span class="fx-sas-label">{t("pairing.sasLabel")}</span>
            <output
              class="fx-sas"
              aria-label={t("pairing.sasLabel")}
              aria-live="polite"
              aria-atomic="true"
            >
              <span>{current().sas.slice(0, 3)}</span>
              <i aria-hidden="true" />
              <span>{current().sas.slice(3)}</span>
            </output>
            <span class="fx-sas-expiry" classList={{ "is-expired": expired() }}>
              {expired()
                ? t("pairing.expired")
                : t("pairing.expiresIn", { time: clock(remaining()) })}
            </span>
          </div>

          <div class="fx-pair-warning">
            <ShieldAlert size={17} aria-hidden="true" />
            <p>{t("pairing.warning")}</p>
          </div>

          <label class="fx-pair-check">
            <input
              type="checkbox"
              checked={acknowledged()}
              disabled={expired() || engine.pairingBusy()}
              onChange={(event) => setAcknowledged(event.currentTarget.checked)}
            />
            <span>{t("pairing.acknowledge")}</span>
          </label>

          <Show when={engine.pairingError()}>
            {(error) => (
              <p class="fx-pair-error" role="alert">
                {t("pairing.requestFailed", { message: error().message })}
              </p>
            )}
          </Show>
          <p class="fx-pair-footnote">{t("pairing.noAuto")}</p>
        </Dialog>
      )}
    </Show>
  );
}
