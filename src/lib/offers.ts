import type { TransferSummary } from "../bindings";

/**
 * The offer the prompt should answer.
 *
 * More than one can be pending - a sender that fires two files inside the same
 * second produces two - and the transfer store is keyed by a hash map, so its
 * iteration order is arbitrary. Picking with `find` meant the dialog could
 * display one offer while the buttons answered another; the loser's sender sat
 * waiting for chunk requests that nothing would ever send.
 *
 * The oldest unpaid offer is the one the dialog has been showing, so it is the
 * one the buttons must answer.
 */
export function oldestOffer(transfers: TransferSummary[]): TransferSummary | null {
  let oldest: TransferSummary | null = null;
  for (const item of transfers) {
    if (item.direction !== "receive" || item.state !== "offered") continue;
    if (oldest === null || item.created_at_ms < oldest.created_at_ms) oldest = item;
  }
  return oldest;
}
