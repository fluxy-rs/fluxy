import type { TransferSummary } from "../../bindings";
import type { useEngine } from "../../state/engine";
import type { useI18n } from "../../i18n";
import * as selectors from "./ChatWorkspace.selectors";

type Engine = ReturnType<typeof useEngine>;
type Translator = ReturnType<typeof useI18n>["t"];

/** The readings a conversation needs, adapted to what each caller asks for. */
export function useChatSelectors(input: {
  engine: Engine;
  t: Translator;
  conversations: () => Parameters<typeof selectors.previewOf>[0];
  transfers: () => TransferSummary[];
  peers: () => Parameters<typeof selectors.nameOf>[0];
}) {
  const displayName = (deviceId: string, fallback: string) =>
    selectors.nameOf(input.peers(), deviceId, fallback);
  const lastPreview = (deviceId: string) =>
    selectors.previewOf(input.conversations(), input.transfers(), deviceId, input.t);
  const trustedPeers = () => selectors.trustedPeers(input.peers());
  const forwardTargets = (transfer: TransferSummary) =>
    selectors.forwardTargets(input.peers(), transfer);

  return { displayName, lastPreview, trustedPeers, forwardTargets };
}
