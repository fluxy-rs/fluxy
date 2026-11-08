import { createSignal } from "solid-js";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { Button, Dialog, toast } from "../ui";

/**
 * First-launch prompt for the receive folder.
 *
 * Android 11 has no way to write to a public folder without the user's grant,
 * so until one is picked a received file could only reach the app's private
 * directory - where no file manager can show it. The prompt therefore stays up
 * until a folder is chosen; the system picker is the only way to satisfy it.
 *
 * The dialog carries no dismiss button on purpose: the offer gate refuses an
 * accept until this is answered, so letting it close would only hide the reason.
 */
export function ReceiveDirDialog() {
  const engine = useEngine();
  const { t } = useI18n();
  const [busy, setBusy] = createSignal(false);

  // Desktop always has a destination, so the flag only goes false on a phone
  // that has not been through the picker yet.
  const open = () => engine.snapshot()?.settings.receive_dir_ready === false;

  async function choose() {
    if (busy()) return;
    setBusy(true);
    try {
      const picked = await engine.pickReceiveDir(t("receiveDir.choose"));
      if (!picked) return;
      // On Android the picker already stored the grant; this refreshes the
      // snapshot with the new label. Desktop keeps the path in its config.
      await engine.setReceiveDir(picked);
      toast.success(t("receiveDir.saved"));
    } catch (cause) {
      toast.error(
        t("receiveDir.failed", {
          message: cause instanceof Error ? cause.message : String(cause),
        }),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      open={open()}
      dismissible={false}
      onOpenChange={() => undefined}
      title={t("receiveDir.title")}
      description={t("receiveDir.body")}
      width="sm"
      footer={
        <Button disabled={busy()} onClick={() => void choose()}>
          {busy() ? t("receiveDir.choosing") : t("receiveDir.action")}
        </Button>
      }
    >
      <p class="text-label text-muted-foreground">{t("receiveDir.hint")}</p>
    </Dialog>
  );
}
