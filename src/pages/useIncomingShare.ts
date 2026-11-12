import { onCleanup, onMount } from "solid-js";
import type { NearbyPeerView, RegisteredFileView } from "../bindings";
import { takeOpenChatDevice } from "../api/engine";
import type { useEngine } from "../state/engine";

type Engine = ReturnType<typeof useEngine>;

/**
 * What another app can push into Fluxy, and what the page does about it.
 *
 * Two channels arrive here: a chat notification deep link (fluxy://chat), and
 * a share from the system sheet, which hands over content URIs and no
 * destination. Neither is delivered by an event on this build, so the queue is
 * drained on mount, on visibility change, and on a poll - a share into the
 * already-foreground app arrives through onNewIntent while document.hidden
 * stays false the whole time, so visibility alone would miss it.
 */
export function useIncomingShare(input: {
  engine: Engine;
  onOpenChat: (deviceId: string) => void;
  onShared: (files: RegisteredFileView[], device: string | null) => void;
  onNearby: (peers: NearbyPeerView[]) => void;
  /// Desktop has neither channel; the shell mounts this everywhere.
  enabled: () => boolean;
}) {
  onMount(() => {
    if (!input.enabled()) return;
    let stopped = false;

    const checkOpenChat = async () => {
      try {
        const deviceId = await takeOpenChatDevice();
        if (deviceId) input.onOpenChat(deviceId);
      } catch {
        // A failed drain is not worth a banner; the next poll tries again.
      }
    };

    const drainShared = async () => {
      try {
        const { uris, device } = await input.engine.sharedFiles();
        if (uris.length === 0) return;
        const registered: RegisteredFileView[] = [];
        for (const uri of uris) {
          const view = await input.engine.openSourceFile(uri);
          if (view) registered.push(view);
        }
        if (registered.length > 0) input.onShared(registered, device);
      } catch (error) {
        console.warn("[share] failed", error);
      }
    };

    const onVisible = () => {
      void checkOpenChat();
      if (!document.hidden) void drainShared();
    };

    const refresh = async () => {
      void checkOpenChat();
      try {
        // Polled rather than event-driven: visibilitychange does not fire
        // reliably when the WebView is restored from the background.
        const peers = await input.engine.nearbyPeers();
        if (!stopped) input.onNearby(peers);
      } catch {
        // The list stays as it was; a transient failure is not worth a banner.
      }
    };

    void checkOpenChat();
    void drainShared();
    void refresh();
    document.addEventListener("visibilitychange", onVisible);
    const timer = window.setInterval(() => {
      void refresh();
      void drainShared();
    }, 2500);

    onCleanup(() => {
      stopped = true;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    });
  });
}
