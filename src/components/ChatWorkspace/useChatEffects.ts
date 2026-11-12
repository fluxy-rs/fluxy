import { createEffect, on, onCleanup, onMount } from "solid-js";
import type { ChatConversationView } from "../../bindings";
import type { useEngine } from "../../state/engine";
import type { useI18n } from "../../i18n";
import { toast } from "../../ui";

type Engine = ReturnType<typeof useEngine>;
type Translator = ReturnType<typeof useI18n>["t"];

/**
 * The things a conversation does without being asked: pick a thread on a wide
 * screen, close one that disappeared, open a session with an online peer, and
 * claim whatever was dropped on the window.
 */
export function useChatEffects(input: {
  engine: Engine;
  t: Translator;
  conversations: () => ChatConversationView[];
  selectedId: () => string | null;
  selected: () => ChatConversationView | undefined;
  compact: () => boolean;
  setCompact: (value: boolean) => void;
  openConversation: (deviceId: string) => void;
  closeConversation: () => void;
}) {
  onMount(() => {
    const query = window.matchMedia("(max-width: 768px)");
    input.setCompact(query.matches);
    const listener = (event: MediaQueryListEvent) => input.setCompact(event.matches);
    query.addEventListener("change", listener);
    onCleanup(() => query.removeEventListener("change", listener));
    // Only a list with nothing open picks a thread for the user. A route that
    // already names a device is an instruction, not a default: opening the
    // first conversation here used to replace it and send the deep link to
    // whoever happened to be at the top of the list.
    if (input.compact() || input.selectedId()) return;
    const first = input.conversations()[0];
    if (first) input.openConversation(first.device_id);
  });

  createEffect(
    on(
      () =>
        input
          .conversations()
          .map((item) => item.device_id)
          .join("|"),
      () => {
        const current = input.selectedId();
        const list = input.conversations();
        if (current && !list.some((item) => item.device_id === current)) {
          input.closeConversation();
        }
      },
    ),
  );

  createEffect(
    on(
      () => {
        const peer = input.selected();
        return peer ? `${peer.device_id}:${peer.online}` : "";
      },
      () => {
        const peer = input.selected();
        if (!peer?.online) return;
        void input.engine.openChat(peer.device_id).catch(() => undefined);
      },
      { defer: true },
    ),
  );

  // Files dropped on the window are handed to the conversation on screen. The
  // engine keeps the last drop until something claims it, so a drop that lands
  // while the chat is closed is still there when it opens.
  createEffect(() => {
    const dropped = input.engine.drop();
    const peer = input.selected();
    if (!dropped || !peer) return;
    input.engine.clearDrop();
    const fileIds = dropped.files.map((file) => file.file_id);
    const folderIds = dropped.folders.map((folder) => folder.folder_id);
    if (fileIds.length === 0 && folderIds.length === 0) return;
    void (async () => {
      try {
        if (fileIds.length > 0) await input.engine.startTransfers(peer.device_id, fileIds);
        for (const folderId of folderIds) {
          await input.engine.startFolder(peer.device_id, folderId);
        }
        const count = fileIds.length + folderIds.length;
        toast.success(
          count === 1
            ? input.t("chat.attachSent", {
                name: dropped.files[0]?.name ?? dropped.folders[0]?.root_name ?? "",
              })
            : input.t("chat.attachSentMany", { count }),
        );
      } catch (cause) {
        toast.error(cause instanceof Error ? cause.message : String(cause));
      }
    })();
  });
}
