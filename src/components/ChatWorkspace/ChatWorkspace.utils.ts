import type { ChatMessageView, TransferSummary } from "../../bindings";
import type { useI18n } from "../../i18n";

type Translator = ReturnType<typeof useI18n>["t"];

/** The sender's word for a transfer state, in the reader's language. */
export function transferLabel(transfer: TransferSummary, t: Translator): string {
  switch (transfer.state) {
    case "completed":
      return t("transfers.state.completed");
    case "failed":
      return t("transfers.state.failed");
    case "paused":
      return t("transfers.state.paused");
    case "interrupted":
      return t("transfers.state.interrupted");
    case "cancelled":
      return t("transfers.state.cancelled");
    case "offered":
      return t("transfers.state.offered");
    case "queued":
      return t("transfers.state.queued");
    case "preparing":
      return t("transfers.state.preparing");
    default:
      return t("transfers.state.transferring");
  }
}

/**
 * The library names its attachment states for the sender's side of an
 * upload; this project names them for both directions.
 */
export function attachmentState(state: string) {
  if (state === "transferring") return "uploading" as const;
  if (state === "failed") return "error" as const;
  return "done" as const;
}

export function failureMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/** What the sender sees under a message: received, sending, delivered, failed. */
export function statusLabel(message: ChatMessageView, t: Translator): string {
  if (message.direction === "received") return t("chat.received");
  switch (message.state) {
    case "sending":
      return t("chat.sending");
    case "delivered":
      return t("chat.delivered");
    case "failed":
      return t("chat.failed");
    default:
      return t("chat.received");
  }
}

/** The protocol's failure codes, spelled out for a person. */
export function errorLabel(message: ChatMessageView, t: Translator): string {
  switch (message.error_code) {
    case "connection_lost":
      return t("chat.errorConnectionLost");
    case "send_timeout":
      return t("chat.errorSendTimeout");
    case "not_delivered":
      return t("chat.errorNotDelivered");
    case "session_closed":
      return t("chat.errorSessionClosed");
    case "transport":
      return t("chat.errorTransport");
    default:
      return message.error_message ?? t("chat.errorGeneric");
  }
}
