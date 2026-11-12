import { createSignal } from "solid-js";
import type { TransferSummary } from "../../bindings";

/**
 * The dialogs a message or a file card can open.
 *
 * They live above the message list, not inside it: a dialog mounted in the
 * scroller would unmount on a re-render and take the scroll position with it.
 */
export function useChatDialogs() {
  const [details, setDetails] = createSignal<TransferSummary | null>(null);
  const [remove, setRemove] = createSignal<TransferSummary | null>(null);
  const [forwardFile, setForwardFile] = createSignal<TransferSummary | null>(null);
  // Forwarding a text needs a destination just like forwarding a file does.
  const [forwardText, setForwardText] = createSignal<string | null>(null);

  return {
    details,
    setDetails,
    remove,
    setRemove,
    forwardFile,
    setForwardFile,
    forwardText,
    setForwardText,
  };
}
