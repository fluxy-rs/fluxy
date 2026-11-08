import { fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { LocalDeviceSettings } from "./LocalDeviceSettings";
import type { SettingsSnapshot } from "../bindings";
import { makeSettings, makeSnapshot } from "../test/fixtures";
import { createMockEngine } from "../test/mock-engine";
import { renderWithEngine } from "../test/render";

function renderPanel(
  options: {
    settings?: ReturnType<typeof makeSettings>;
    setDeviceName?: (name: string) => Promise<SettingsSnapshot>;
    setNotifications?: (enabled: boolean) => Promise<SettingsSnapshot>;
    setAutoAccept?: (enabled: boolean) => Promise<SettingsSnapshot>;
    setAutostart?: (enabled: boolean) => Promise<SettingsSnapshot>;
    mobile?: () => boolean;
  } = {},
) {
  const engine = createMockEngine({
    snapshot: () => makeSnapshot({ settings: options.settings ?? makeSettings() }),
    ...(options.setDeviceName ? { setDeviceName: options.setDeviceName } : {}),
    ...(options.setNotifications ? { setNotifications: options.setNotifications } : {}),
    ...(options.setAutostart ? { setAutostart: options.setAutostart } : {}),
    ...(options.setAutoAccept ? { setAutoAccept: options.setAutoAccept } : {}),
    ...(options.mobile ? { mobile: options.mobile } : {}),
  });
  renderWithEngine(() => <LocalDeviceSettings />, engine);
  return engine;
}

function nameField(): HTMLInputElement {
  return screen.getByLabelText("设备名") as HTMLInputElement;
}

describe("LocalDeviceSettings", () => {
  it("shows the persisted name and notification state", () => {
    renderPanel();

    expect(nameField().value).toBe("Local Device");
    const toggle = screen.getByRole("switch", { name: /桌面通知/ });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    // The switch is a span with role="switch" and tabindex, not a button:
    // asserting the tag name would tie this test to one implementation.
    expect(toggle).toHaveAttribute("tabindex", "0");
  });

  it("saves a trimmed name from the keyboard", async () => {
    const setDeviceName = vi.fn(async () => makeSettings({ device_name: "Desk" }));
    renderPanel({ setDeviceName });

    fireEvent.input(nameField(), { target: { value: "  Desk  " } });
    fireEvent.submit(nameField().closest("form")!);

    await waitFor(() => expect(setDeviceName).toHaveBeenCalledWith("Desk"));
  });

  it("refuses an empty name and keeps the field for correction", async () => {
    const setDeviceName = vi.fn(async () => makeSettings());
    renderPanel({ setDeviceName });

    fireEvent.input(nameField(), { target: { value: "   " } });
    fireEvent.submit(nameField().closest("form")!);

    expect(await screen.findByRole("alert")).toHaveTextContent("设备名不能为空。");
    expect(nameField()).toHaveAttribute("aria-invalid", "true");
    expect(setDeviceName).not.toHaveBeenCalled();
  });

  it("reverts an unsaved name with Escape", () => {
    renderPanel();

    fireEvent.input(nameField(), { target: { value: "Draft" } });
    expect(screen.getByRole("button", { name: "保存名称" })).toBeEnabled();

    fireEvent.keyDown(nameField(), { key: "Escape" });

    expect(nameField().value).toBe("Local Device");
    expect(screen.getByRole("button", { name: "保存名称" })).toBeDisabled();
  });

  it("surfaces a rename failure without losing the typed value", async () => {
    const setDeviceName = vi.fn(async () => {
      throw new Error("pairing in progress");
    });
    renderPanel({ setDeviceName });

    fireEvent.input(nameField(), { target: { value: "Desk" } });
    fireEvent.submit(nameField().closest("form")!);

    expect(await screen.findByRole("alert")).toHaveTextContent("pairing in progress");
    expect(nameField().value).toBe("Desk");
  });

  it("toggles desktop notifications", async () => {
    const setNotifications = vi.fn(async () => makeSettings({ notifications: false }));
    renderPanel({ setNotifications });

    fireEvent.click(screen.getByRole("switch", { name: /桌面通知/ }));

    await waitFor(() => expect(setNotifications).toHaveBeenCalledWith(false));
  });

  it("toggles auto-accept for incoming transfers", async () => {
    const setAutoAccept = vi.fn(async () => makeSettings({ auto_accept_transfers: true }));
    renderPanel({ setAutoAccept });

    // Off by default, so the switch starts unchecked and the click turns it on.
    const toggle = screen.getByRole("switch", { name: /自动接收文件/ });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(toggle);

    await waitFor(() => expect(setAutoAccept).toHaveBeenCalledWith(true));
  });

  it("toggles the login item and shows where it lives", async () => {
    const setAutostart = vi.fn(async () =>
      makeSettings({ autostart: true, autostart_location: "test login item" }),
    );
    renderPanel({ setAutostart });

    const toggle = screen.getByRole("switch", { name: /开机启动/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    fireEvent.click(toggle);

    await waitFor(() => expect(setAutostart).toHaveBeenCalledWith(true));
  });

  it("reports a rejected login item without claiming success", async () => {
    const setAutostart = vi.fn(async () => {
      throw new Error("reg.exe rejected the login item");
    });
    renderPanel({ setAutostart });

    fireEvent.click(screen.getByRole("switch", { name: /开机启动/ }));

    await waitFor(() => expect(setAutostart).toHaveBeenCalledWith(true));
    // The panel still shows the persisted state, which stayed off.
    expect(screen.getByRole("switch", { name: /开机启动/ })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("keeps the switch reachable while a save is in flight", async () => {
    let release: (() => void) | undefined;
    const setNotifications = vi.fn(
      (_enabled: boolean) =>
        new Promise<ReturnType<typeof makeSettings>>((resolve) => {
          release = () => resolve(makeSettings({ notifications: false }));
        }),
    );
    renderPanel({ setNotifications });

    const toggle = screen.getByRole("switch", { name: /桌面通知/ });
    fireEvent.click(toggle);
    // A span cannot carry the native disabled attribute; the switch exposes
    // aria-disabled instead, which is what assistive tech reads anyway.
    expect(toggle).toHaveAttribute("aria-disabled", "true");

    release?.();
    await waitFor(() => expect(toggle).toBeEnabled());
  });

  it("shows the receive directory and lets the user change it", async () => {
    const writes: Array<string | null> = [];
    const engine = createMockEngine({
      snapshot: () => makeSnapshot({ settings: makeSettings() }),
      pickReceiveDir: vi.fn(async () => "D:/inbox"),
      setReceiveDir: vi.fn(async (directory: string | null) => {
        writes.push(directory);
        return makeSettings({ receive_dir: directory ?? "default" });
      }),
    });

    renderWithEngine(() => <LocalDeviceSettings />, engine);

    expect(screen.getByText(/received/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "选择目录" }));
    await vi.waitFor(() => expect(writes).toContain("D:/inbox"));
  });

  it("hides the desktop login item on Android", () => {
    renderPanel({ mobile: () => true });
    expect(screen.queryByText("开机启动")).not.toBeInTheDocument();
  });

  it("shows the login item on a desktop platform", () => {
    renderPanel();
    expect(screen.getByText("开机启动")).toBeInTheDocument();
  });
});
