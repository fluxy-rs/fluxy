import { For, Show } from "solid-js";
import type { PeerSummary, RegisteredFileView } from "../bindings";
import { useI18n } from "../i18n";
import { Button, Dialog } from "../ui";

/**
 * Where a shared file goes.
 *
 * A share from another app arrives with content URIs and nothing else: no
 * destination, no file dialog, no obvious next step. The app used to open on a
 * page that listed nothing and offered nothing to press, which reads as a
 * broken share. This asks the one question that is left - which device - and
 * can be dismissed without sending anything.
 */
export function ShareTargetsDialog(props: {
  files: RegisteredFileView[] | null;
  peers: PeerSummary[];
  busy: boolean;
  onSend: (deviceId: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const targets = () => props.peers.filter((peer) => peer.trusted && peer.online);

  return (
    <Dialog
      open={props.files !== null}
      onOpenChange={(open) => {
        if (!open) props.onClose();
      }}
      title={t("mobile.shareTitle")}
      description={t("mobile.shareBody")}
      width="sm"
      footer={
        <Button variant="ghost" onClick={() => props.onClose()}>
          {t("common.cancel")}
        </Button>
      }
    >
      <div class="flex flex-col gap-2">
        <p class="text-label text-muted-foreground">
          {t("mobile.shareCount", { count: props.files?.length ?? 0 })}
        </p>
        <Show
          when={targets().length > 0}
          fallback={<p class="text-body-sm text-muted-foreground">{t("mobile.shareEmpty")}</p>}
        >
          <ul class="flex flex-col gap-1">
            <For each={targets()}>
              {(peer) => (
                <li>
                  <Button
                    variant="ghost"
                    class="w-full justify-start"
                    disabled={props.busy}
                    onClick={() => props.onSend(peer.device_id)}
                  >
                    {peer.name}
                  </Button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </div>
    </Dialog>
  );
}
