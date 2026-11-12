import type { ChatMessageView, TransferAction, TransferSummary } from "../../bindings";
import { toast } from "../../ui";
import type { useEngine } from "../../state/engine";
import type { useI18n } from "../../i18n";
import { failureMessage } from "./ChatWorkspace.utils";

type Engine = ReturnType<typeof useEngine>;
type Translator = ReturnType<typeof useI18n>["t"];

/// Hand a finished receive to the platform's default app.
///
/// The call used to be voided, so a refusal from the platform - the file is
/// gone, nothing is registered for the type, the installer will not run -
/// looked exactly like a card that does nothing when clicked.
export async function openReceived(engine: Engine, t: Translator, transfer: TransferSummary) {
  try {
    await engine.openReceivedTransfer(transfer.transfer_id);
  } catch (cause) {
    const message = failureMessage(cause);
    // The installer refuses to start until the user lets Fluxy install apps;
    // the plugin answers with a code so the copy stays here.
    toast.error(
      message.includes("install_permission_required")
        ? t("chat.openNeedsInstallPermission")
        : t("chat.openFailed", { message }),
    );
  }
}

export async function revealFile(engine: Engine, t: Translator, transfer: TransferSummary) {
  try {
    await engine.revealTransfer(transfer.transfer_id);
  } catch (cause) {
    toast.error(t("transfers.revealFailed", { message: failureMessage(cause) }));
  }
}

export async function deleteFile(engine: Engine, t: Translator, transfer: TransferSummary) {
  try {
    await engine.deleteTransferFile(transfer.transfer_id);
    toast.success(t("transfers.deleted", { name: transfer.file_name }));
  } catch (cause) {
    toast.error(t("transfers.deleteFailed", { message: failureMessage(cause) }));
  }
}

export async function forwardFile(
  engine: Engine,
  t: Translator,
  transfer: TransferSummary,
  deviceId: string,
) {
  try {
    await engine.forwardTransfer(transfer.transfer_id, deviceId);
    toast.success(t("transfers.forwardStarted", { name: transfer.file_name }));
  } catch (cause) {
    toast.error(t("transfers.forwardFailed", { message: failureMessage(cause) }));
  }
}

export async function forwardText(engine: Engine, t: Translator, deviceId: string, text: string) {
  try {
    await engine.sendChat(deviceId, text);
    toast.success(t("chat.forwarded"));
  } catch (cause) {
    toast.error(t("chat.forwardFailed", { message: failureMessage(cause) }));
  }
}

export async function copyText(t: Translator, text: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(t("chat.copied"));
  } catch {
    toast.error(t("chat.copyFailed"));
  }
}

/// Lets one message be selected, then selects it so the handles appear.
///
/// Selection is disabled application-wide, so the class has to be in place
/// before the range is built - the browser will not select text it is told is
/// unselectable, and the style change lands on the next frame.
export function selectText(message: ChatMessageView, onSelectable: (id: string | null) => void) {
  onSelectable(message.message_id);
  requestAnimationFrame(() => {
    const element = document.querySelector<HTMLElement>(
      `[data-message-id="${message.message_id}"]`,
    );
    if (!element) return;
    const range = document.createRange();
    range.selectNodeContents(element);
    const selection = window.getSelection();
    if (!selection) return;
    selection.removeAllRanges();
    selection.addRange(range);
  });
}

export async function sendText(engine: Engine, deviceId: string, text: string) {
  await engine.sendChat(deviceId, text);
}

export function controlTransfer(engine: Engine, transfer: TransferSummary, action: TransferAction) {
  void engine.controlTransfer(transfer.transfer_id, action).catch((cause: unknown) => {
    toast.error(cause instanceof Error ? cause.message : String(cause));
  });
}

export async function retryMessage(engine: Engine, deviceId: string, messageId: string) {
  await engine.retryChat(deviceId, messageId);
}

/// Attach files to a conversation.
///
/// A phone has no file dialog the desktop UI can drive, so it uses the platform
/// picker and gets a descriptor back; the desktop opens its own. The platform
/// decides this, not the IPC bridge: both builds expose __TAURI_INTERNALS__, so
/// engine.desktop is true on Android too.
export async function attachFiles(engine: Engine, t: Translator, deviceId: string) {
  const onAndroid = engine.snapshot()?.status.platform === "android";
  if (onAndroid) {
    // The platform picker hands back one descriptor, so a phone sends one file
    // per tap; the desktop dialog can return a whole selection.
    const file = await engine.pickSourceFile();
    if (!file) return;
    await engine.startTransfers(deviceId, [file.file_id]);
    toast.success(t("chat.attachSent", { name: file.name }));
    return;
  }
  const files = await engine.pickFiles(t("chat.attachTitle"));
  if (files.length === 0) return;
  await engine.startTransfers(
    deviceId,
    files.map((file) => file.file_id),
  );
  toast.success(
    files.length === 1
      ? t("chat.attachSent", { name: files[0].name })
      : t("chat.attachSentMany", { count: files.length }),
  );
}
