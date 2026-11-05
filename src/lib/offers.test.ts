import { describe, expect, it } from "vitest";
import { oldestOffer } from "./offers";
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
