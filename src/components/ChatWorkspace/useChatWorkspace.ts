import { createMemo, createSignal } from "solid-js";
import type { TransferSummary } from "../../bindings";
import { useEngine } from "../../state/engine";
import { useI18n } from "../../i18n";
import { createCompact } from "../../lib/viewport";
import type { MenuTarget } from "./ChatWorkspace.types";
import * as commands from "./ChatWorkspace.commands";
import { buildTimeline } from "./ChatWorkspace.selectors";
import { useChatDialogs } from "./useChatDialogs";
import { useChatSelectors } from "./useChatSelectors";
import { createChatActions } from "./useChatActions";
import { useChatEffects } from "./useChatEffects";
import { createChatMenu } from "./useChatMenu";

/**
 * The state a conversation route hands to the workspace.
 *
 * The open thread is controlled by the caller rather than read from the
 * router: a route is how the app expresses "this thread is open" - it gives
 * the phone's back button somewhere to go - but the workspace stays a plain
 * component, which keeps it testable without a router around it.
 */
export function useChatWorkspace(props: {
  selectedId?: string;
  onSelect?: (deviceId: string | null) => void;
}) {
  const engine = useEngine();
  const { t } = useI18n();

  const conversations = () => engine.chatConversations();
  const selectedId = () => props.selectedId ?? null;
  const openConversation = (deviceId: string) => props.onSelect?.(deviceId);
  const closeConversation = () => props.onSelect?.(null);

  // A phone-shaped viewport cannot show a conversation list and a thread side
  // by side: 218px of list squeezed the messages into a letterbox. Compact mode
  // swaps one for the other, the way a chat app does, instead of stacking two
  // scroll areas inside a square screen. The test is the window, not the
  // platform: a car head unit is Android with room for both columns.
  const compact = createCompact();
  const [hiddenMessages, setHiddenMessages] = createSignal<readonly string[]>([]);
  const [selectableMessage, setSelectableMessage] = createSignal<string | null>(null);

  const dialogs = useChatDialogs();

  const selected = createMemo(() => {
    const list = conversations();
    const id = selectedId();
    if (!id) {
      // Desktop keeps a thread open by default; a compact screen starts on the
      // list so the user picks who to talk to.
      return compact() ? undefined : list[0];
    }
    return list.find((item) => item.device_id === id) ?? (compact() ? undefined : list[0]);
  });

  const messages = createMemo(() => engine.chatMessages());
  const transfers = createMemo(() => engine.transfers());
  const peers = createMemo(() => engine.snapshot()?.peers ?? []);

  const visibleMessages = createMemo(() => {
    const peer = selected();
    if (!peer) return [];
    const hidden = hiddenMessages();
    return messages().filter(
      (message) => message.device_id === peer.device_id && !hidden.includes(message.message_id),
    );
  });

  const timeline = createMemo(() => {
    const peer = selected();
    return peer ? buildTimeline(visibleMessages(), transfers(), peer.device_id) : [];
  });

  const readings = useChatSelectors({ engine, t, conversations, transfers, peers });
  const { displayName, lastPreview, trustedPeers, forwardTargets } = readings;

  const actions = createChatActions({
    engine,
    t,
    selectedDeviceId: () => selected()?.device_id,
    selectedOnline: () => Boolean(selected()?.online),
    onCloseDelete: () => dialogs.setRemove(null),
    onCloseForwardFile: () => dialogs.setForwardFile(null),
    onCloseForwardText: () => dialogs.setForwardText(null),
    onSelectable: setSelectableMessage,
  });

  const menu = createChatMenu({
    t,
    hiddenMessages,
    onHideMessage: (messageId) => setHiddenMessages([...hiddenMessages(), messageId]),
    onDetails: dialogs.setDetails,
    onDeleteFile: dialogs.setRemove,
    onForwardFile: dialogs.setForwardFile,
    onForwardText: dialogs.setForwardText,
    onCopy: (message) => void actions.copy(message),
    onSelectText: (message) => actions.select(message),
    onClearSelection: () => setSelectableMessage(null),
  });

  useChatEffects({
    engine,
    t,
    conversations,
    selectedId,
    selected,
    compact,
    openConversation,
    closeConversation,
  });

  const onControl = (
    transfer: TransferSummary,
    action: Parameters<typeof engine.controlTransfer>[1],
  ) => commands.controlTransfer(engine, transfer, action);

  return {
    engine,
    compact,
    conversations,
    selectedId,
    selected,
    displayName,
    lastPreview,
    timeline,
    trustedPeers,
    forwardTargets,
    openConversation,
    closeConversation,
    actions,
    menu: {
      target: menu.target as () => MenuTarget | null,
      setTarget: menu.setTarget,
      anchor: menu.anchor,
      actions: menu.actions,
      onFileLongPress: menu.onFileLongPress,
      onTextLongPress: menu.onTextLongPress,
    },
    dialogs,
    selection: { selectable: selectableMessage, setSelectable: setSelectableMessage },
    onControl,
  };
}
