import { createMemo, createSignal } from "solid-js";
import { Check, X } from "lucide-solid";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, toast } from "../ui";
import { formatBytes } from "../lib/format";

/**
 * The accept/reject prompt for an incoming file.
 *
 * This used to live inside the transfers page, which meant an offer only
 * produced a prompt while that page happened to be mounted. A file sent to a
 * phone sitting on the conversation list therefore arrived, waited, and expired
 * with nothing on screen to answer it. The prompt is a property of the
 * connection, not of a page, so it is mounted once in the shell.
 */
export function IncomingOfferDialog() {
  const engine = useEngine();
  const { t } = useI18n();

  const [pending, setPending] = createSignal<string | null>(null);

  const incoming = createMemo(() => engine.incomingOffer());

  async function respond(accepted: boolean) {
    const offer = incoming();
    if (!offer || pending()) return;
    setPending(offer.transfer_id);
    try {
      await engine.respondTransfer(offer.transfer_id, accepted);
      toast.success(
        accepted
          ? t("transfers.acceptedToast", { name: offer.file_name })
          : t("transfers.rejectedToast", { name: offer.file_name }),
      );
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setPending(null);
    }
  }

  return (
    <Dialog
      open={incoming() !== null}
      // Only the two buttons answer this: the sender stays blocked until the
      // offer is accepted or rejected, so nothing else may close it.
      dismissible={false}
      onOpenChange={() => undefined}
      title={t("transfers.offerTitle")}
      description={
        incoming()
          ? t("transfers.offerBody", {
              name: incoming()!.device_name,
              file: incoming()!.file_name,
              size: formatBytes(incoming()!.bytes_total),
            })
          : undefined
      }
      width="sm"
      footer={
        <>
          <Button
            variant="outline"
            disabled={pending() === incoming()?.transfer_id}
            onClick={() => void respond(false)}
            icon={<X size={14} />}
          >
            {t("transfers.reject")}
          </Button>
          <Button
            disabled={pending() === incoming()?.transfer_id}
            onClick={() => void respond(true)}
            icon={<Check size={14} />}
          >
            {t("transfers.accept")}
          </Button>
        </>
      }
    >
      <p class="text-xs text-muted-foreground">{t("transfers.offerNote")}</p>
    </Dialog>
  );
}
