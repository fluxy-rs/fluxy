import { For, Show } from "solid-js";
import type { PeerSummary, TransferSummary } from "../../bindings";
import { useI18n } from "../../i18n";
import { Button, Dialog } from "../../ui";
import { formatBytes, formatClock } from "../../lib/format";
import { transferLabel } from "./ChatWorkspace.utils";

/**
 * The four dialogs the conversation can raise.
 *
 * They live outside the message list on purpose: a dialog mounted inside the
 * scroller would unmount with a re-render and take the scroll position with it.
 */
export function ChatDialogs(props: {
  peers: PeerSummary[];
  forwardTextValue: string | null;
  onCloseForwardText: () => void;
  onForwardText: (deviceId: string) => void;
  details: TransferSummary | null;
  onCloseDetails: () => void;
  remove: TransferSummary | null;
  onCloseRemove: () => void;
  onConfirmRemove: () => void;
  forwardFile: TransferSummary | null;
  onCloseForwardFile: () => void;
  onForwardFile: (transfer: TransferSummary, deviceId: string) => void;
  forwardTargets: (transfer: TransferSummary) => PeerSummary[];
  displayName: (deviceId: string, fallback: string) => string;
}) {
  const { t, locale } = useI18n();

  return (
    <>
      <Dialog
        open={props.forwardTextValue !== null}
        onOpenChange={(open) => {
          if (!open) props.onCloseForwardText();
        }}
        title={t("transfers.forwardTitle")}
        description={t("chat.forwardTextBody")}
      >
        <ul class="flex flex-col gap-1">
          <For each={props.peers}>
            {(peer) => (
              <li>
                <Button
                  variant="ghost"
                  class="w-full justify-start"
                  onClick={() => props.onForwardText(peer.device_id)}
                >
                  {props.displayName(peer.device_id, peer.name)}
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Dialog>

      <Dialog
        open={props.details !== null}
        onOpenChange={(open) => {
          if (!open) props.onCloseDetails();
        }}
        title={t("transfers.detailsTitle")}
      >
        <Show when={props.details}>
          {(transfer) => (
            <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-body-sm">
              <dt class="text-muted-foreground">{t("transfers.detailsFile")}</dt>
              <dd class="break-all">{transfer().file_name}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsSize")}</dt>
              <dd class="font-mono">{formatBytes(transfer().bytes_total)}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsDirection")}</dt>
              <dd>
                {transfer().direction === "send"
                  ? t("transfers.sendDirection")
                  : t("transfers.receiveDirection")}
              </dd>
              <dt class="text-muted-foreground">{t("transfers.detailsPeer")}</dt>
              <dd>{props.displayName(transfer().device_id, transfer().device_name)}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsState")}</dt>
              <dd>{transferLabel(transfer(), t)}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsCreated")}</dt>
              <dd class="font-mono">{formatClock(transfer().created_at_ms, locale())}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsUpdated")}</dt>
              <dd class="font-mono">{formatClock(transfer().updated_at_ms, locale())}</dd>
              <dt class="text-muted-foreground">{t("transfers.detailsTransferId")}</dt>
              <dd class="font-mono text-xs break-all">{transfer().transfer_id}</dd>
            </dl>
          )}
        </Show>
      </Dialog>

      <Dialog
        open={props.remove !== null}
        onOpenChange={(open) => {
          if (!open) props.onCloseRemove();
        }}
        title={t("transfers.deleteTitle")}
        description={t("transfers.deleteBody", { name: props.remove?.file_name ?? "" })}
        footer={
          <>
            <Button variant="ghost" onClick={() => props.onCloseRemove()}>
              {t("transfers.cancel")}
            </Button>
            <Button variant="destructive" onClick={() => props.onConfirmRemove()}>
              {t("transfers.deleteConfirm")}
            </Button>
          </>
        }
      />

      <Dialog
        open={props.forwardFile !== null}
        onOpenChange={(open) => {
          if (!open) props.onCloseForwardFile();
        }}
        title={t("transfers.forwardTitle")}
        description={t("transfers.forwardBody", { name: props.forwardFile?.file_name ?? "" })}
      >
        <Show
          when={props.forwardFile}
          fallback={<p class="text-body-sm text-muted-foreground">{t("transfers.forwardEmpty")}</p>}
        >
          {(transfer) => (
            <ul class="flex flex-col gap-1">
              <For each={props.forwardTargets(transfer())}>
                {(peer) => (
                  <li>
                    <Button
                      variant="ghost"
                      class="w-full justify-start"
                      onClick={() => props.onForwardFile(transfer(), peer.device_id)}
                    >
                      {props.displayName(peer.device_id, peer.name)}
                    </Button>
                  </li>
                )}
              </For>
            </ul>
          )}
        </Show>
      </Dialog>
    </>
  );
}
