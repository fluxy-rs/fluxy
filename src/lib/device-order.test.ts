import { describe, expect, it } from "vitest";

import { orderDevices, type OrderablePeer } from "./device-order";

function peer(over: Partial<OrderablePeer> & { device_id: string; name: string }): OrderablePeer {
  return { trusted: true, online: true, paired_at_ms: 1000, ...over };
}

const noMessages = () => 0;

describe("orderDevices", () => {
  it("puts paired and reachable first, then discoverable, then offline", () => {
    const rows = [
      peer({ device_id: "c", name: "Offline", trusted: true, online: false }),
      peer({ device_id: "b", name: "Stranger", trusted: false }),
      peer({ device_id: "a", name: "Mine", trusted: true, online: true }),
    ];
    expect(orderDevices(rows, noMessages, "en").map((row) => row.name)).toEqual([
      "Mine",
      "Stranger",
      "Offline",
    ]);
  });

  it("orders the reachable tier by last message, newest first", () => {
    const rows = [
      peer({ device_id: "a", name: "Quiet" }),
      peer({ device_id: "b", name: "Chatty" }),
      peer({ device_id: "c", name: "Recent" }),
    ];
    const times: Record<string, number> = { a: 100, b: 900, c: 500 };
    expect(orderDevices(rows, (id) => times[id] ?? 0, "en").map((row) => row.name)).toEqual([
      "Chatty",
      "Recent",
      "Quiet",
    ]);
  });

  it("falls back to the pairing time when nothing was ever said", () => {
    const rows = [
      peer({ device_id: "a", name: "Old", paired_at_ms: 100 }),
      peer({ device_id: "b", name: "New", paired_at_ms: 900 }),
    ];
    expect(orderDevices(rows, noMessages, "en").map((row) => row.name)).toEqual(["New", "Old"]);
  });

  it("orders the other two tiers by name", () => {
    const rows = [
      peer({ device_id: "a", name: "Zeta", trusted: false }),
      peer({ device_id: "b", name: "Alpha", trusted: false }),
      peer({ device_id: "c", name: "Yankee", trusted: true, online: false }),
      peer({ device_id: "d", name: "Bravo", trusted: true, online: false }),
    ];
    expect(orderDevices(rows, noMessages, "en").map((row) => row.name)).toEqual([
      "Alpha",
      "Zeta",
      "Bravo",
      "Yankee",
    ]);
  });

  it("does not mutate the array it was given", () => {
    const rows = [peer({ device_id: "a", name: "Z" }), peer({ device_id: "b", name: "A" })];
    const before = rows.map((row) => row.name);
    orderDevices(rows, noMessages, "en");
    expect(rows.map((row) => row.name)).toEqual(before);
  });
});
