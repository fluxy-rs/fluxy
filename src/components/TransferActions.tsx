import { Show } from "solid-js";
import type { TransferSummary } from "../bindings";
import { useI18n } from "../i18n";
import type { TransferAction } from "../bindings";

/**
 * The actions a transfer row offers in its current state.
 *
 * The runtime has always had pause, resume, cancel and retry; the card only
 * ever showed a clock, so a file picked by mistake could not be stopped and a
 * paused transfer could not be restarted from the conversation it was sent in.
 * Keeping the choices in one place means every surface that shows a transfer
 * offers the same ones.
 */
export function TransferActions(props: {
  transfer: TransferSummary;
  /** An action is already in flight for this transfer. */
  busy: boolean;
  onAction: (action: TransferAction) => void;
}) {
  const { t } = useI18n();
  const state = () => props.transfer.state;
  const moving = () => state() === "transferring" || state() === "preparing";
  // `offered` matters most: a sender waiting for the other side to accept is
  // exactly the case where the file was picked by mistake and nothing appears
  // to be happening. `interrupted` is a restored checkpoint nobody continued.
  const cancellable = () =>
    moving() ||
    state() === "paused" ||
    state() === "queued" ||
    state() === "offered" ||
    state() === "interrupted";
  const retryable = () => state() === "failed" || state() === "cancelled";

  const button = (label: string, action: TransferAction, destructive = false) => (
    <button
      type="button"
      class="text-xs underline-offset-2 hover:underline disabled:opacity-50 disabled:hover:no-underline"
      classList={{ "text-primary": !destructive, "text-destructive": destructive }}
      disabled={props.busy}
      onClick={() => props.onAction(action)}
    >
      {label}
    </button>
  );

  return (
    <span class="flex items-center gap-2">
      <Show when={moving()}>{button(t("transfers.pause"), "pause")}</Show>
      <Show when={state() === "paused"}>{button(t("transfers.resume"), "resume")}</Show>
      <Show when={retryable()}>{button(t("transfers.retry"), "retry")}</Show>
      <Show when={cancellable()}>{button(t("transfers.cancel"), "cancel", true)}</Show>
    </span>
  );
}
