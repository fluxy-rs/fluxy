import { describe, expect, it } from "vitest";
import { failureReason, failureReasonKey } from "./transfer-errors";

describe("transfer failure reasons", () => {
  it("resolves a known protocol code in both locales", () => {
    expect(failureReasonKey("disk_full")).toBe("transfers.reason.disk_full");
    expect(failureReason("disk_full", "zh-CN", "?")).toContain("磁盘空间不足");
    expect(failureReason("disk_full", "en", "?")).toContain("Disk full");
    expect(failureReason("source_changed", "en", "?")).toContain(
      "Source changed",
    );
  });

  it("names a rejection as a rejection, not as a generic failure", () => {
    // The sender records state=failed with code=rejected and code_number=3001,
    // so anything that keys off the state or the number shows a bare "failed".
    // The reason has to come from the string code.
    expect(failureReasonKey("rejected")).toBe("transfers.reason.rejected");
    expect(failureReason("rejected", "zh-CN", "失败")).toContain("拒绝");
    expect(failureReason("rejected", "en", "failed")).toContain("declined");
  });

  it("falls back for a missing or unknown code", () => {
    expect(failureReasonKey(null)).toBeNull();
    expect(failureReasonKey(undefined)).toBeNull();
    expect(failureReasonKey("")).toBeNull();
    // A peer from the future may report a code this build does not know.
    expect(failureReasonKey("future_code")).toBeNull();
    expect(failureReason("future_code", "zh-CN", "未记录原因")).toBe(
      "未记录原因",
    );
  });
});
