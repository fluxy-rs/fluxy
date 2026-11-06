import type { TransferSummary } from "../bindings";

/**
 * Every offer the prompt should answer, oldest batch first.
 *
 * A sender that picks several files at once produces one offer per file, all
 * arriving within a few milliseconds of each other. Showing a prompt per file
 * makes the receiver answer the same question N times, and each answer leaves
 * the next sender blocked until it is given, so the queue never drains on its
 * own.
 *
 * Offers that belong to one batch are grouped by `batch_id`. Files picked
 * without a batch carry none, so they fall back to the device: everything
 * pending from one peer at the same moment was sent as one gesture.
 *
 * The transfer store is a hash map, so its iteration order is arbitrary. The
 * batch is chosen by the oldest offer inside it, which is the one the dialog
 * has been showing, and the members are ordered by that same stamp so the list
 * does not shuffle between renders.
 */
export function pendingOffers(transfers: TransferSummary[]): TransferSummary[] {
  const pending = transfers.filter(
    (item) => item.direction === "receive" && item.state === "offered",
  );
  if (pending.length === 0) return [];

  const key = (item: TransferSummary) => item.batch_id ?? `device:${item.device_id}`;

  // The group whose oldest member is oldest wins, so a batch that has been
  // waiting does not get starved by a newer one arriving behind it.
  let chosen: string | null = null;
  let chosenAt = Number.POSITIVE_INFINITY;
  for (const item of pending) {
    if (item.created_at_ms < chosenAt) {
      chosenAt = item.created_at_ms;
      chosen = key(item);
    }
  }

  return pending
    .filter((item) => key(item) === chosen)
    .sort((a, b) => a.created_at_ms - b.created_at_ms);
}

/** The oldest pending offer, kept for callers that only ask for one. */
export function oldestOffer(transfers: TransferSummary[]): TransferSummary | null {
  return pendingOffers(transfers)[0] ?? null;
}
