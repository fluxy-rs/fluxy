/**
 * How the device list is ordered.
 *
 * Three tiers, in this order: paired and reachable, discoverable but not
 * paired, paired but offline. The first tier is ordered by when you last
 * talked to the device; the other two by name.
 *
 * The reasoning: the device you are actually using belongs at the top, and
 * "actually using" means recent traffic, not a recent pairing. An unpaired
 * device is only in the list while it is discoverable, so the middle tier is
 * where a device you could pair with belongs - ahead of one you already trust
 * but cannot reach. A name is what is left to order the unused and the
 * unreachable by.
 */

/** The fields this ordering needs; PeerSummary satisfies it. */
export interface OrderablePeer {
  device_id: string;
  name: string;
  trusted: boolean;
  online: boolean;
  paired_at_ms: number;
}

export function orderDevices<T extends OrderablePeer>(
  peers: readonly T[],
  lastMessageAt: (deviceId: string) => number,
  locale: string,
): T[] {
  const tier = (peer: OrderablePeer): number => {
    if (peer.trusted) return peer.online ? 0 : 2;
    // Unpaired peers only appear while discovery sees them, so they are
    // always reachable and always belong in the middle.
    return 1;
  };

  return [...peers].sort((a, b) => {
    const tierA = tier(a);
    const tierB = tier(b);
    if (tierA !== tierB) return tierA - tierB;

    if (tierA === 0) {
      // A pairing nobody has messaged yet falls back to when it was made, so
      // a device paired a minute ago is not buried under older conversations.
      const lastA = lastMessageAt(a.device_id) || a.paired_at_ms;
      const lastB = lastMessageAt(b.device_id) || b.paired_at_ms;
      if (lastA !== lastB) return lastB - lastA;
    }

    return a.name.localeCompare(b.name, locale);
  });
}
