import { createSignal } from "solid-js";
import { useNavigate } from "@solidjs/router";
import type { RegisteredFileView } from "../bindings";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { toast } from "../ui";
import { ShareTargetsDialog } from "./ShareTargetsDialog";
import { useIncomingShare } from "../pages/useIncomingShare";

/**
 * Where something shared into Fluxy goes.
 *
 * A share arrives as a pending intent, not an event, and does not reopen a
 * running app - so this sits above the routes. It also owns the chooser: the
 * user should be asked who to send to, not dropped on the device list where
 * the file they just picked is nowhere to be seen.
 */
export function IncomingShareBridge() {
  const engine = useEngine();
  const { t } = useI18n();
  const navigate = useNavigate();
  const [files, setFiles] = createSignal<RegisteredFileView[] | null>(null);
  const [busy, setBusy] = createSignal(false);

  async function sendTo(deviceId: string) {
    const shared = files();
    if (!shared || busy()) return;
    setBusy(true);
    try {
      await engine.startTransfers(
        deviceId,
        shared.map((file) => file.file_id),
      );
      toast.success(t("mobile.sent"));
      setFiles(null);
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  useIncomingShare({
    engine,
    enabled: () => engine.mobile(),
    onOpenChat: (deviceId) => navigate(`/chat/${deviceId}`),
    onShared: (shared, device) => {
      // A link that named a device sends without asking. Otherwise the share
      // has a file and no destination, so ask - here, over whatever the user
      // was looking at.
      if (device) {
        setFiles(shared);
        void sendTo(device);
        return;
      }
      setFiles(shared);
    },
    onNearby: () => {},
  });

  return (
    <ShareTargetsDialog
      files={files()}
      peers={engine.snapshot()?.peers ?? []}
      busy={busy()}
      onSend={(deviceId) => void sendTo(deviceId)}
      onClose={() => setFiles(null)}
    />
  );
}
