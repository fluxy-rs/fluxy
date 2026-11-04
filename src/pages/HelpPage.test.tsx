import { fireEvent, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { HelpPage } from "./HelpPage";
import { makeNetwork, makeSettings, makeSnapshot } from "../test/fixtures";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

function renderHelp(overrides: {
  network?: Parameters<typeof makeNetwork>[0];
  settings?: Parameters<typeof makeSettings>[0];
} = {}) {
  const engine = createMockEngine({
    snapshot: () =>
      makeSnapshot({
        settings: makeSettings(overrides.settings),
        network: makeNetwork(overrides.network),
      }),
  });
  renderWithEngine(() => <HelpPage />, engine);
  return engine;
}

describe("HelpPage", () => {
  it("shows the discovery port and the port the listener actually bound", () => {
    renderHelp({ network: { listen_addr: "192.168.1.20:53111" } });

    expect(screen.getByText("UDP 42420")).toBeInTheDocument();
    expect(screen.getByText("UDP 53111")).toBeInTheDocument();
  });

  it("falls back to the configured port when no listener is bound yet", () => {
    renderHelp({
      network: { listen_addr: null },
      settings: { listen_port: 53000 },
    });

    expect(screen.getByText("UDP 53000")).toBeInTheDocument();
  });

  it("writes the live ports into the copyable platform rules", () => {
    renderHelp({ network: { listen_addr: "192.168.1.20:53111" } });

    const windows = screen.getByText(/-LocalPort 42420/, {
      exact: false,
    });
    expect(windows.textContent).toContain("-LocalPort 53111");
    expect(screen.getByText(/ufw allow 53111\/udp/)).toBeInTheDocument();
    // The FAQ answers substitute the discovery port too.
    expect(screen.getByText(/都允许了 UDP 42420/)).toBeInTheDocument();
  });

  it("copies the platform rule and reports the result", async () => {
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderHelp({ network: { listen_addr: "192.168.1.20:53111" } });

    fireEvent.click(screen.getAllByRole("button", { name: "复制命令" })[0]);

    await vi.waitFor(() => expect(writeText).toHaveBeenCalled());
    expect(String(writeText.mock.calls[0][0])).toContain("-LocalPort 53111");
  });

  it("reports the login item instead of claiming background startup", () => {
    renderHelp({
      settings: {
        autostart: true,
        autostart_location: "test login item",
      },
    });

    expect(screen.getByText(/test login item/)).toBeInTheDocument();
    expect(screen.queryByText(/开机启动已关闭/)).not.toBeInTheDocument();
  });

  it("warns when discovery is neither advertising nor browsing", () => {
    renderHelp({ network: { advertised: false, browsing: false } });

    expect(
      screen.getByText(/附近设备可能无法发现本机/),
    ).toBeInTheDocument();
  });
});
