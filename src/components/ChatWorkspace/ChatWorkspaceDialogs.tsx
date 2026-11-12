import { ChatDialogs } from "./ChatDialogs";
import type { useChatWorkspace } from "./useChatWorkspace";

/**
 * The dialogs a conversation can raise, wired to the workspace.
 *
 * They are kept out of the thread's own JSX because a dialog rendered inside
 * the scroller unmounts with a re-render and takes the scroll position with
 * it; keeping their wiring here also keeps the shell readable.
 */
export function ChatWorkspaceDialogs(props: { workspace: ReturnType<typeof useChatWorkspace> }) {
  const workspace = () => props.workspace;

  return (
    <ChatDialogs
      peers={workspace().trustedPeers()}
      forwardTextValue={workspace().dialogs.forwardText()}
      onCloseForwardText={() => workspace().dialogs.setForwardText(null)}
      onForwardText={(deviceId) => {
        const text = workspace().dialogs.forwardText();
        if (text) void workspace().actions.forwardTextTo(text, deviceId);
      }}
      details={workspace().dialogs.details()}
      onCloseDetails={() => workspace().dialogs.setDetails(null)}
      remove={workspace().dialogs.remove()}
      onCloseRemove={() => workspace().dialogs.setRemove(null)}
      onConfirmRemove={() => {
        const target = workspace().dialogs.remove();
        if (target) void workspace().actions.remove(target);
      }}
      forwardFile={workspace().dialogs.forwardFile()}
      onCloseForwardFile={() => workspace().dialogs.setForwardFile(null)}
      onForwardFile={(transfer, deviceId) =>
        void workspace().actions.forwardFileTo(transfer, deviceId)
      }
      forwardTargets={workspace().forwardTargets}
      displayName={workspace().displayName}
    />
  );
}
