import { describe, expect, it } from "vitest";
import { oldestOffer, pendingOffers } from "./offers";
import type { TransferSummary } from "../bindings";

function offer(id: string, createdAt: number, overrides: Partial<TransferSummary> = {}) {
  return {
    transfer_id: id,
    device_id: "peer",
    device_name: "Peer",
    direction: "receive",
    state: "offered",
    file_name: "file.bin",
    bytes_total: 100,
    bytes_done: 0,
    created_at_ms: createdAt,
    ...overrides,
  } as TransferSummary;
}

describe("oldestOffer", () => {
  it("returns nothing when no offer is pending", () => {
    expect(oldestOffer([])).toBeNull();
    expect(oldestOffer([offer("a", 1, { state: "completed" })])).toBeNull();
    expect(oldestOffer([offer("a", 1, { direction: "send" })])).toBeNull();
  });

  it("picks the oldest pending offer regardless of list order", () => {
    const older = offer("older", 100);
    const newer = offer("newer", 200);
    expect(oldestOffer([older, newer])?.transfer_id).toBe("older");
    expect(oldestOffer([newer, older])?.transfer_id).toBe("older");
  });

  it("ignores finished transfers when an older one is still pending", () => {
    const done = offer("done", 50, { state: "completed" });
    const pending = offer("pending", 150);
    expect(oldestOffer([done, pending])?.transfer_id).toBe("pending");
  });
});

describe("pendingOffers", () => {
  it("answers every file of one batch together", () => {
    const first = offer("a", 100, { batch_id: "batch" });
    const second = offer("b", 101, { batch_id: "batch" });
    expect(pendingOffers([first, second]).map((item) => item.transfer_id)).toEqual(["a", "b"]);
  });

  it("does not mix two batches, and picks the one that has waited longer", () => {
    const older = offer("old", 50, { batch_id: "one" });
    const newer = offer("new", 900, { batch_id: "two" });
    expect(pendingOffers([newer, older]).map((item) => item.transfer_id)).toEqual(["old"]);
  });

  it("groups plain multi-select offers by device when there is no batch", () => {
    const a = offer("a", 10);
    const b = offer("b", 11);
    const other = offer("c", 20, { device_id: "someone-else" });
    expect(pendingOffers([b, other, a]).map((item) => item.transfer_id)).toEqual(["a", "b"]);
  });

  it("ignores offers that are no longer pending", () => {
    const done = offer("done", 1, { state: "completed" });
    const sent = offer("sent", 2, { direction: "send" });
    expect(pendingOffers([done, sent])).toEqual([]);
  });
});
