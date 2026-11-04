import { fireEvent, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import type { NetworkStatusView } from "../bindings";
import { NetworkSettings } from "./NetworkSettings";
import { makeNetwork, makeSnapshot } from "../test/fixtures";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

function renderPanel(
  overrides: Partial<NetworkStatusView> = {},
  engineOverrides: Parameters<typeof createMockEngine>[0] = {},
) {
  const engine = createMockEngine({
    snapshot: () => makeSnapshot({ network: makeNetwork(overrides) }),
    ...engineOverrides,
  });
  renderWithEngine(() => <NetworkSettings />, engine);
  return engine;
}

/** Move the panel to manual mode and pick an adapter. */
function pick(label: string) {
  fireEvent.click(screen.getByRole("button", { name: "手动" }));
  fireEvent.click(screen.getByRole("button", { name: new RegExp(label) }));
  fireEvent.click(screen.getByRole("button", { name: "应用" }));
}

describe("NetworkSettings", () => {
  it("explains the active selection and marks the recommended adapter", () => {
    renderPanel();

    expect(screen.getByText(/已把套接字绑定到该网卡/)).toBeInTheDocument();
    expect(screen.getByText("推荐")).toBeInTheDocument();
    expect(screen.getByText("使用中")).toBeInTheDocument();
    expect(screen.getByText("192.168.1.20")).toBeInTheDocument();
    expect(screen.getByText("10.0.0.5")).toBeInTheDocument();
    expect(screen.getByText(/优先默认路由接口/)).toBeInTheDocument();
    // The interface list is a local view; the panel says so.
    expect(screen.getByText(/该列表只在本机显示与记录/)).toBeInTheDocument();
  });

  it("asks for confirmation and sends the manual pick", async () => {
    const engine = renderPanel();

    pick("Wi-Fi");

    expect(screen.getByText(/切换会断开当前连接/)).toBeInTheDocument();
    expect(screen.getByText(/对端需要重新发现本机/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /切换并重绑/ }));

    await vi.waitFor(() =>
      expect(engine.setNetworkSelection).toHaveBeenCalledWith(
        "manual",
        "Wi-Fi",
        "10.0.0.5",
      ),
    );
  });

  it("turns a failed switch into actionable guidance", async () => {
    renderPanel({}, {
      setNetworkSelection: vi.fn(async (): Promise<NetworkStatusView> => {
        throw new Error(
          "network_unavailable: the chosen address 10.0.0.5 on Wi-Fi is gone",
        );
      }),
    });

    pick("Wi-Fi");
    fireEvent.click(screen.getByRole("button", { name: /切换并重绑/ }));

    await vi.waitFor(() =>
      expect(screen.getByText(/如何恢复/)).toBeInTheDocument(),
    );
    expect(
      screen.getByText(/最近一次错误：network_unavailable/),
    ).toBeInTheDocument();
    expect(screen.getByText(/选择一块网卡/)).toBeInTheDocument();
    // The proposed selection survives, so the user can retry or adjust it.
    expect(screen.getByRole("button", { name: "应用" })).toBeEnabled();
  });

  it("guides a lost manual address instead of blaming discovery", () => {
    renderPanel({
      state: "unavailable",
      selection_mode: "manual",
      detail: "the chosen address 10.0.0.5 on Wi-Fi is gone",
    });

    expect(screen.getByText(/所选地址已不存在/)).toBeInTheDocument();
    expect(
      screen.getByText(/the chosen address 10.0.0.5 on Wi-Fi is gone/),
    ).toBeInTheDocument();
  });

  it("warns when the platform could not pin the socket", () => {
    renderPanel({ interface_pinned: false });

    expect(screen.getByText(/未固定套接字到网卡/)).toBeInTheDocument();
  });

  it("marks virtual and disconnected adapters", () => {
    renderPanel({
      interface_options: [
        {
          name: "vEthernet (Default Switch)",
          label: "vEthernet (Default Switch)",
          kind: "ethernet",
          index: 22,
          up: true,
          virtual_adapter: true,
          default_route: true,
          addresses: ["172.20.16.1"],
          recommended: false,
        },
        {
          name: "Ethernet",
          label: "Ethernet",
          kind: "ethernet",
          index: 7,
          up: false,
          virtual_adapter: false,
          default_route: false,
          addresses: ["169.254.210.249"],
          recommended: false,
        },
      ],
    });

    expect(screen.getByText("虚拟/VPN")).toBeInTheDocument();
    expect(screen.getByText("未连接")).toBeInTheDocument();
  });

  it("keeps adapter rows inert while automatic selection is active", () => {
    renderPanel();

    expect(screen.getByRole("button", { name: /Wi-Fi/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "应用" })).toBeDisabled();
  });

  it("copies the interface list and addresses for support", async () => {
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderPanel();

    fireEvent.click(screen.getByRole("button", { name: "复制接口与地址" }));

    await vi.waitFor(() => expect(writeText).toHaveBeenCalled());
    const text = String(writeText.mock.calls[0][0]);
    expect(text).toContain("mode=auto");
    expect(text).toContain("pinned=true");
    expect(text).toContain("Wi-Fi");
    expect(text).toContain("192.168.1.20");
  });
});
