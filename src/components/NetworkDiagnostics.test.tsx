import { screen } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { NetworkDiagnostics } from "./NetworkDiagnostics";
import { makeNetwork, makeSnapshot } from "../test/fixtures";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

describe("NetworkDiagnostics", () => {
  it("shows the fallback gateway, targets, and counters", () => {
    const engine = createMockEngine({
      snapshot: () =>
        makeSnapshot({
          network: makeNetwork({
            fallback_peers: 1,
            fallback_probes_sent: 7,
            fallback_responses: 3,
          }),
        }),
    });

    renderWithEngine(() => <NetworkDiagnostics />, engine);

    expect(screen.getByText(/网关探测与广播已启用/)).toBeInTheDocument();
    expect(screen.getByText(/探针 7 \/ 回复 3/)).toBeInTheDocument();
    expect(screen.getByText("192.168.1.1")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.255:42420")).toBeInTheDocument();
    expect(screen.getByText("255.255.255.255:42420")).toBeInTheDocument();
  });

  it("surfaces a fallback error without hiding mDNS state", () => {
    const engine = createMockEngine({
      snapshot: () =>
        makeSnapshot({
          network: makeNetwork({
            fallback_active: false,
            fallback_last_error: "probe socket unavailable",
          }),
        }),
    });

    renderWithEngine(() => <NetworkDiagnostics />, engine);

    expect(screen.getByText(/未启用/)).toBeInTheDocument();
    expect(
      screen.getByText("回退通道错误：probe socket unavailable"),
    ).toBeInTheDocument();
    expect(screen.getByText("发现中")).toBeInTheDocument();
  });
});
