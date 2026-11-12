import { createSignal } from "solid-js";
import { Copy, Info, Share2, TextSelect, Trash2 } from "lucide-solid";
import type { ChatMessageView, TransferSummary } from "../../bindings";
import type { useI18n } from "../../i18n";
import type { MessageAction } from "../MessageActions";
import type { MenuAnchor, MenuTarget } from "./ChatWorkspace.types";

type Translator = ReturnType<typeof useI18n>["t"];

/**
 * The long-press menu: one panel, two kinds of target.
 *
 * A file card offers details, forward and delete; a text bubble offers copy,
 * forward, select-text and delete. They share the entrance, not the items.
 */
export function createChatMenu(input: {
  t: Translator;
  hiddenMessages: () => readonly string[];
  onHideMessage: (messageId: string) => void;
  onDetails: (transfer: TransferSummary) => void;
  onDeleteFile: (transfer: TransferSummary) => void;
  onForwardFile: (transfer: TransferSummary) => void;
  onForwardText: (text: string) => void;
  onCopy: (message: ChatMessageView) => void;
  onSelectText: (message: ChatMessageView) => void;
  onClearSelection: () => void;
}) {
  const [target, setTarget] = createSignal<MenuTarget | null>(null);
  const [anchor, setAnchor] = createSignal<MenuAnchor | null>(null);

  /// The point the menu hangs from: the horizontal centre and the top edge of
  /// the pressed element, which keeps the panel over the message it describes.
  const anchorOf = (event: PointerEvent): MenuAnchor => {
    // `currentTarget` is cleared once dispatch ends, and the timer fires later.
    const element = event.target as HTMLElement | null;
    const rect = element?.getBoundingClientRect();
    return rect
      ? { x: rect.left + rect.width / 2, y: rect.top }
      : { x: event.clientX, y: event.clientY };
  };

  const actions = (): MessageAction[] => {
    const current = target();
    if (!current) return [];
    const forward = (onSelect: () => void): MessageAction => ({
      label: input.t("chat.forward"),
      icon: Share2,
      onSelect,
    });
    const remove = (onSelect: () => void): MessageAction => ({
      label: input.t("chat.delete"),
      icon: Trash2,
      destructive: true,
      onSelect,
    });

    if (current.kind === "file") {
      return [
        {
          label: input.t("chat.details"),
          icon: Info,
          onSelect: () => input.onDetails(current.transfer),
        },
        forward(() => input.onForwardFile(current.transfer)),
        remove(() => input.onDeleteFile(current.transfer)),
      ];
    }
    return [
      {
        label: input.t("chat.copy"),
        icon: Copy,
        onSelect: () => input.onCopy(current.message),
      },
      forward(() => input.onForwardText(current.message.text)),
      {
        label: input.t("chat.selectText"),
        icon: TextSelect,
        onSelect: () => input.onSelectText(current.message),
      },
      remove(() => input.onHideMessage(current.message.message_id)),
    ];
  };

  const onFileLongPress = (event: PointerEvent, transfer: TransferSummary) => {
    setAnchor(anchorOf(event));
    setTarget({ kind: "file", transfer });
  };

  const onTextLongPress = (event: PointerEvent, message: ChatMessageView) => {
    // A new menu means a new target: the previous message gives its selection back.
    input.onClearSelection();
    setAnchor(anchorOf(event));
    setTarget({ kind: "text", message });
  };

  return { target, setTarget, anchor, setAnchor, actions, onFileLongPress, onTextLongPress };
}
