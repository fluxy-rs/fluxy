import { createSignal } from "solid-js";
import { toast } from "../../ui";
import type { ChatMessageView, TransferSummary } from "../../bindings";
import type { useEngine } from "../../state/engine";
import type { useI18n } from "../../i18n";
import * as commands from "./ChatWorkspace.commands";
import { failureMessage } from "./ChatWorkspace.utils";

type Engine = ReturnType<typeof useEngine>;
type Translator = ReturnType<typeof useI18n>["t"];

/**
 * What the conversation can do to the world: send, attach, retry, forward,
 * open, reveal, delete, copy, select.
 *
 * Each one closes the dialog it was raised from, so a failed action cannot
 * leave a modal open with no way to tell what happened.
 */
export function createChatActions(input: {
  engine: Engine;
  t: Translator;
  selectedDeviceId: () => string | undefined;
  selectedOnline: () => boolean;
  onCloseDelete: () => void;
  onCloseForwardFile: () => void;
  onCloseForwardText: () => void;
  onSelectable: (messageId: string | null) => void;
}) {
  const [draft, setDraft] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [retrying, setRetrying] = createSignal<string | null>(null);

  const canSend = () => input.selectedOnline() && !sending() && draft().trim().length > 0;

  async function send() {
    const peer = input.selectedDeviceId();
    const text = draft().trim();
    if (!peer || !text || !input.selectedOnline() || sending()) return;
    setSending(true);
    try {
      await commands.sendText(input.engine, peer, text);
      setDraft("");
    } catch (cause) {
      toast.error(input.t("chat.sendFailed", { message: failureMessage(cause) }));
    } finally {
      setSending(false);
    }
  }

  async function retry(message: ChatMessageView) {
    const peer = input.selectedDeviceId();
    if (!peer || retrying()) return;
    setRetrying(message.message_id);
    try {
      await commands.retryMessage(input.engine, peer, message.message_id);
    } catch (cause) {
      toast.error(input.t("chat.retryFailed", { message: failureMessage(cause) }));
    } finally {
      setRetrying(null);
    }
  }

  async function attach() {
    const peer = input.selectedDeviceId();
    if (!peer) return;
    try {
      await commands.attachFiles(input.engine, input.t, peer);
    } catch (cause) {
      toast.error(failureMessage(cause));
    }
  }

  async function remove(transfer: TransferSummary) {
    await commands.deleteFile(input.engine, input.t, transfer);
    input.onCloseDelete();
  }

  async function forwardFileTo(transfer: TransferSummary, deviceId: string) {
    await commands.forwardFile(input.engine, input.t, transfer, deviceId);
    input.onCloseForwardFile();
  }

  async function forwardTextTo(text: string, deviceId: string) {
    await commands.forwardText(input.engine, input.t, deviceId, text);
    input.onCloseForwardText();
  }

  return {
    draft,
    setDraft,
    sending,
    canSend,
    retrying,
    send,
    retry,
    attach,
    remove,
    forwardFileTo,
    forwardTextTo,
    openReceived: (transfer: TransferSummary) =>
      commands.openReceived(input.engine, input.t, transfer),
    revealFile: (transfer: TransferSummary) => commands.revealFile(input.engine, input.t, transfer),
    copy: (message: ChatMessageView) => commands.copyText(input.t, message.text),
    select: (message: ChatMessageView) => commands.selectText(message, input.onSelectable),
  };
}
