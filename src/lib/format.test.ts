import { describe, expect, it } from "vitest";
import { etaSeconds, formatBytes, formatEta, formatPercent, formatSpeed } from "./format";

describe("formatBytes", () => {
  it("scales to binary multiples", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
  });

  it("drops the fraction once the mantissa is wide", () => {
    expect(formatBytes(10 * 1024 * 1024)).toBe("10.0 MB");
    expect(formatBytes(150 * 1024 * 1024)).toBe("150 MB");
    expect(formatBytes(2 * 1024 ** 4)).toBe("2.0 TB");
  });
});

describe("formatSpeed", () => {
  it("appends the rate unit and hides unknown rates", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(-5)).toBe("—");
    expect(formatSpeed(1024)).toBe("1.0 KB/s");
    expect(formatSpeed(2 * 1024 * 1024)).toBe("2.0 MB/s");
  });
});

describe("formatPercent", () => {
  it("rounds and clamps to the 0..100 range", () => {
    expect(formatPercent(0, 0)).toBe(0);
    expect(formatPercent(0, 2048)).toBe(0);
    expect(formatPercent(512, 2048)).toBe(25);
    expect(formatPercent(2048, 2048)).toBe(100);
    expect(formatPercent(4096, 2048)).toBe(100);
  });
});

describe("etaSeconds and formatEta", () => {
  it("needs a positive rate and a known total", () => {
    expect(etaSeconds(0, 1024, 0)).toBeNull();
    expect(etaSeconds(0, 0, 1024)).toBeNull();
    expect(etaSeconds(512, 2048, 512)).toBe(3);
    expect(etaSeconds(2048, 2048, 512)).toBe(0);
  });

  it("formats seconds, minutes, and hours", () => {
    expect(formatEta(null)).toBe("—");
    expect(formatEta(0)).toBe("—");
    expect(formatEta(2)).toBe("2s");
    expect(formatEta(59.2)).toBe("60s");
    expect(formatEta(65)).toBe("1m 5s");
    expect(formatEta(3700)).toBe("1h 1m");
  });
});
