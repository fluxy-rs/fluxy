import { Show } from "solid-js";
import { MessageCircle, Paperclip } from "lucide-solid";
import { useI18n } from "../../i18n";
import { MessageActions } from "../MessageActions";
import { ChatComposer } from "./ChatComposer";
import { ChatWorkspaceDialogs } from "./ChatWorkspaceDialogs";
import { ChatHeader } from "./ChatHeader";
import { ChatTimeline } from "./ChatTimeline";
import { ConversationList } from "./ConversationList";
import { useChatWorkspace } from "./useChatWorkspace";

/**
 * A conversation: who to talk to, what has been said, and a place to say more.
 *
 * Sending a file happens here rather than on the transfers page - the two
 * belong together, and switching pages to attach something is the whole point
 * of the complaint this exists for.
 */
export function ChatWorkspace(props: {
  selectedId?: string;
  onSelect?: (deviceId: string | null) => void;
}) {
  const { t } = useI18n();
  const workspace = useChatWorkspace(props);
  const peer = () => workspace.selected();
  const peerName = () => {
    const current = peer();
    return current ? workspace.displayName(current.device_id, current.name) : "";
  };

  return (
    <>
      <section
        // `grid-cols-1` is load-bearing: a grid with no template sizes its
        // implicit track to the content, and a truncated file name is as wide
        // as the whole name, which stretched the column past the viewport and
        // hid half the conversation behind the section's clip.
        class="relative grid grid-cols-1 min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-card"
        classList={{ "grid-cols-[minmax(15rem,18.75rem)_minmax(0,1fr)]": !workspace.compact() }}
        onPointerDown={(event: PointerEvent) => {
          const id = workspace.selection.selectable();
          if (!id) return;
          const target = event.target as HTMLElement | null;
          if (!target?.closest(`[data-message-id="${id}"]`)) {
            workspace.selection.setSelectable(null);
          }
        }}
        aria-label={t("chat.title")}
      >
        <Show when={!workspace.compact() || !peer()}>
          <ConversationList
            conversations={workspace.conversations()}
            selectedId={peer()?.device_id ?? null}
            onSelect={(deviceId) => workspace.openConversation(deviceId)}
            displayName={workspace.displayName}
            lastPreview={workspace.lastPreview}
          />
        </Show>

        <Show when={!workspace.compact() || peer()}>
          <Show
            when={peer()}
            fallback={
              <div class="flex flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
                <MessageCircle size={26} aria-hidden="true" />
                <h2 class="text-title font-[620] tracking-[-0.012em]">{t("chat.selectTitle")}</h2>
                <p class="text-body-sm text-muted-foreground">{t("chat.selectBody")}</p>
              </div>
            }
          >
            {(current) => (
              <div class="flex min-h-0 flex-col">
                <ChatHeader
                  name={peerName()}
                  online={current().online}
                  connected={current().session_state === "connected"}
                  compact={workspace.compact()}
                  onBack={() => workspace.closeConversation()}
                />
                <ChatTimeline
                  deviceId={current().device_id}
                  entries={workspace.timeline()}
                  busyTransferId={workspace.engine.transferBusy()}
                  retryingMessageId={workspace.actions.retrying()}
                  selectableMessageId={workspace.selection.selectable()}
                  peerName={peerName()}
                  peerOnline={current().online}
                  desktopMenuDisabled={!workspace.engine.desktop || workspace.engine.mobile()}
                  canForward={(transfer) => workspace.forwardTargets(transfer).length > 0}
                  onOpenFile={(transfer) => void workspace.actions.openReceived(transfer)}
                  onRevealFile={(transfer) => void workspace.actions.revealFile(transfer)}
                  onDeleteFile={(transfer) => workspace.dialogs.setRemove(transfer)}
                  onDetailsFile={(transfer) => workspace.dialogs.setDetails(transfer)}
                  onForwardFile={(transfer) => workspace.dialogs.setForwardFile(transfer)}
                  onFileLongPress={(event, transfer) =>
                    workspace.menu.onFileLongPress(event, transfer)
                  }
                  onTextLongPress={(event, message) =>
                    workspace.menu.onTextLongPress(event, message)
                  }
                  onControl={(transfer, action) => workspace.onControl(transfer, action)}
                  onRetry={(message) => void workspace.actions.retry(message)}
                />
                <ChatComposer
                  online={current().online}
                  draft={workspace.actions.draft()}
                  sending={workspace.actions.sending()}
                  canSend={workspace.actions.canSend()}
                  onDraft={workspace.actions.setDraft}
                  onSend={() => void workspace.actions.send()}
                  onAttach={() => void workspace.actions.attach()}
                />
              </div>
            )}
          </Show>
        </Show>

        {/* The OS owns the drag; this is only the invitation to drop. */}
        <Show when={workspace.engine.dragActive() && peer()}>
          <div class="pointer-events-none absolute inset-0 z-10 grid place-items-center rounded-lg border-2 border-dashed border-primary/60 bg-background/80">
            <div class="flex flex-col items-center gap-1.5">
              <Paperclip size={22} class="text-primary" aria-hidden="true" />
              <p class="text-body font-medium">{t("chat.dropHere")}</p>
            </div>
          </div>
        </Show>
      </section>

      <MessageActions
        open={workspace.menu.target() !== null}
        anchor={workspace.menu.anchor()}
        onClose={() => workspace.menu.setTarget(null)}
        actions={workspace.menu.actions()}
        label={t("chat.actions")}
      />

      <ChatWorkspaceDialogs workspace={workspace} />
    </>
  );
}
